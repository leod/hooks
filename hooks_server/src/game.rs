use std::collections::BTreeMap;
use std::mem;

use rand::{self, Rng};

use bit_manager::BitWriter;

use shred::{Fetch, RunNow};

use hooks_util::timer::{Stopwatch, Timer};
use hooks_util::profile;
use hooks_common::{self, event, game, GameInfo, LeaveReason, PlayerId, PlayerInfo, PlayerInput,
                   TickDeltaNum, TickNum};
use hooks_common::net::protocol::ClientGameMsg;
use hooks_common::registry::Registry;
use hooks_common::repl::{player, tick};

use host::{self, Host};

struct Player {
    /// Tick in which this player joined the game.
    join_tick: TickNum,

    /// Last tick that we know the player has received.
    last_ack_tick: Option<TickNum>,

    /// Last tick that has been started by the player.
    last_started_tick: Option<TickNum>,

    /// We keep a history of ticks for each player, to be used as a basis for delta encoding.
    /// We delta encode w.r.t. to `last_ack_tick`.
    tick_history: tick::History<game::EntitySnapshot>,

    /// Events queued only for this player for the next tick. We currently use this to inform newly
    /// joined players of existing players, with a stack of `PlayerJoined` events.
    queued_events: event::Sink,

    /// Inputs received from the client.
    queued_inputs: BTreeMap<TickNum, PlayerInput>,

    /// Last input that has been executed from this client, if any.
    last_input_num: Option<TickNum>,
}

impl Player {
    pub fn new(join_tick: TickNum, event_reg: event::Registry) -> Player {
        Player {
            join_tick: join_tick,
            last_ack_tick: None,
            last_started_tick: None,
            tick_history: tick::History::new(event_reg),
            queued_events: event::Sink::new(),
            queued_inputs: BTreeMap::new(),
            last_input_num: None,
        }
    }
}

pub struct Game {
    game_state: game::State,
    game_runner: game::run::AuthRunner,

    players: BTreeMap<PlayerId, Player>,

    /// Timer to start the next tick.
    tick_timer: Timer,

    /// Number of tick we will start next.
    next_tick: TickNum,

    /// Stopwatch for advancing timers.
    update_stopwatch: Stopwatch,

    /// Events queued for the next tick.
    queued_events: event::Sink,

    /// Reusable buffer for serialization.
    write_buffer: Vec<u8>,
}

fn register(reg: &mut Registry, game_info: &GameInfo) {
    hooks_common::auth::register(reg, game_info);
}

impl Game {
    pub fn new(game_info: GameInfo) -> Game {
        let mut game_state = {
            let mut reg = Registry::new();

            register(&mut reg, &game_info);

            game::State::from_registry(reg)
        };

        game::init::auth::create_state(&mut game_state.world);

        Game {
            game_state,
            game_runner: game::run::AuthRunner::new(),
            players: BTreeMap::new(),
            tick_timer: Timer::new(game_info.tick_duration()),
            next_tick: 0,
            update_stopwatch: Stopwatch::new(),
            queued_events: event::Sink::new(),
            write_buffer: Vec::new(),
        }
    }

    pub fn game_info(&self) -> Fetch<GameInfo> {
        self.game_state.world.read_resource::<GameInfo>()
    }

    pub fn update(&mut self, host: &mut Host) -> Result<(), host::Error> {
        // 1. Advance timers
        {
            let duration = self.update_stopwatch.get_reset();
            self.tick_timer += duration;
        }

        // 2. Detect players that are lagged too far behind
        for (&player_id, player) in self.players.iter() {
            let num_delta = if let Some(last_ack_tick) = player.last_ack_tick {
                assert!(last_ack_tick < self.next_tick);
                self.next_tick - last_ack_tick
            } else {
                // Player has not acknowledged a tick yet
                self.next_tick - player.join_tick + 1
            };

            if num_delta > TickDeltaNum::max_value() as TickNum {
                // NOTE: In the future, if we have a higher tick rate, it might be better to send
                //       a full snapshot to players who are lagged too far behind to use delta
                //       encoding. Then, a different mechanism will need to be used to force
                //       disconnect lagged clients.
                info!(
                    "Player {}'s last acknowledged tick is {} ticks (ca. {:?}) in the past. \
                     Forcefully disconnecting.",
                    player_id,
                    num_delta,
                    self.tick_timer.period() * num_delta
                );
                host.force_disconnect(player_id, LeaveReason::Lagged)?;
            }
        }

        // 3. Handle network events and create resulting game events
        while let Some(event) = host.service()? {
            match event {
                host::Event::PlayerJoined(player_id, name) => {
                    assert!(!self.players.contains_key(&player_id));

                    let player_info = PlayerInfo::new(name.clone());

                    // At the start of the next tick, all players will receive an event that a new
                    // player has joined. This induces the repl player management on server and
                    // clients --- including the newly connected client.
                    self.queued_events.push(player::JoinedEvent {
                        id: player_id,
                        info: player_info,
                    });

                    let mut player = Player::new(self.next_tick, self.game_state.event_reg.clone());

                    // Send additional `JoinedEvent`s only for the new player, in the first tick
                    // that it receives
                    self.send_player_list(&mut player);

                    self.players.insert(player_id, player);
                }
                host::Event::PlayerLeft(player_id, reason) => {
                    assert!(self.players.contains_key(&player_id));

                    // Inform game state on server and clients of player leaving
                    self.queued_events.push(player::LeftEvent {
                        id: player_id,
                        reason,
                    });

                    self.players.remove(&player_id);
                }
                host::Event::ClientGameMsg(player_id, msg) => {
                    assert!(self.players.contains_key(&player_id));

                    match msg {
                        ClientGameMsg::PlayerInput(_input) => {
                            panic!("not used right now");
                        }
                        ClientGameMsg::ReceivedTick(tick_num) => {
                            // Client has acknowledged a tick
                            let player = self.players.get_mut(&player_id).unwrap();

                            if tick_num >= self.next_tick {
                                // Invalid tick number! Forcefully disconnect the client.
                                // NOTE: The corresponding `host::Event::PlayerLeft` event will be
                                //       handled in the next iteration of the while loop.
                                warn!(
                                    "Player {} says he received tick {}, but we will start\
                                     tick {} next, disconnecting",
                                    player_id, tick_num, self.next_tick
                                );

                                host.force_disconnect(player_id, LeaveReason::InvalidMsg)?;
                                continue;
                            }

                            // Since game messages are unreliable, it is possible that we receive
                            // acknowledgements out of order
                            if tick_num > player.last_ack_tick.unwrap_or(0) {
                                player.last_ack_tick = Some(tick_num);

                                // Now we do not need snapshots from ticks older than that anymore.
                                // The server will always try to delta encode with respect to the
                                // last tick acknowledged by the client.
                                player.tick_history.prune_older_ticks(tick_num);

                                // We should have data for every tick in
                                // [player.last_ack_tick, self.next_tick - 1]
                                assert!(player.tick_history.get(tick_num).is_some());

                                if player
                                    .tick_history
                                    .get(tick_num)
                                    .as_ref()
                                    .unwrap()
                                    .snapshot
                                    .is_none()
                                {
                                    warn!(
                                        "Player {} has acknowledged the non-snapshot tick {},\
                                         disconnecting",
                                        player_id, tick_num
                                    );

                                    host.force_disconnect(player_id, LeaveReason::InvalidMsg)?;
                                    continue;
                                }
                            }
                        }
                        ClientGameMsg::StartedTick(started_tick_num, player_input) => {
                            if started_tick_num >= self.next_tick {
                                // Got input for a tick that we haven't even started yet!
                                warn!(
                                    "Player {} says he started tick {}, but we will start\
                                     tick {} next, disconnecting",
                                    player_id, started_tick_num, self.next_tick
                                );

                                host.force_disconnect(player_id, LeaveReason::InvalidMsg)?;
                                continue;
                            }

                            let player = self.players.get_mut(&player_id).unwrap();

                            player.last_started_tick = Some(match player.last_started_tick {
                                Some(last_started_tick) => {
                                    if last_started_tick == started_tick_num {
                                        // Player started tick twice!
                                        warn!(
                                            "Player {} started tick {} twice, disconnecting",
                                            player_id, started_tick_num
                                        );

                                        host.force_disconnect(player_id, LeaveReason::InvalidMsg)?;
                                        continue;
                                    } else {
                                        // Input might have been received out of order
                                        last_started_tick.max(started_tick_num)
                                    }
                                }
                                None => started_tick_num,
                            });

                            // TODO: Ignore player input that is too old

                            player
                                .queued_inputs
                                .insert(started_tick_num, player_input.clone());
                        }
                    }
                }
            }
        }

        // 4. Run a tick periodically
        if self.tick_timer.trigger() {
            profile!("tick");

            // 4.1. Run tick
            //debug!("Starting tick {}", self.next_tick);

            // Here, the state's `event::Sink` is empty. Push all the events that we have queued.
            assert!(
                self.game_state
                    .world
                    .read_resource::<event::Sink>()
                    .is_empty()
            );
            self.game_state.push_events(self.queued_events.clear());

            // For now, just run everyone's queued inputs. This will need to be refined!
            let inputs = self.players
                .iter()
                .flat_map(|(&player_id, player)| {
                    player
                        .queued_inputs
                        .iter()
                        .map(|(_tick_num, input)| (player_id, input.clone()))
                        .collect::<Vec<_>>()
                })
                .collect();
            for player in self.players.values_mut() {
                // Remember the last input we run (i.e. the maximal number contained in the map)
                let max_queued_num = player.queued_inputs.iter().next_back().map(|(&num, _)| num);
                if let Some(max_queued_num) = max_queued_num {
                    // This if here is important, since otherwise we forget the player's last input
                    // num if we don't have a queued input for a tick.
                    player.last_input_num = Some(max_queued_num);
                }

                player.queued_inputs.clear();
            }

            let tick_events = {
                profile!("run");
                self.game_runner.run_tick(&mut self.game_state, inputs)
            };

            // Can unwrap here, since replication errors should at most happen on the client-side
            let tick_events = tick_events.unwrap();

            // 4.2. Record tick in history and send snapshots for every player
            profile!("send");

            let entity_classes = self.game_state.world.read_resource::<game::EntityClasses>();
            let send_snapshot = self.next_tick % self.game_info().ticks_per_snapshot == 0;

            for (&player_id, player) in self.players.iter_mut() {
                // Events for this player are the special queued events as well as the shared
                // events of this tick
                let mut player_events = mem::replace(&mut player.queued_events, event::Sink::new());
                for event in &tick_events {
                    player_events.push_box(event.clone_event());
                }

                let snapshot = if send_snapshot {
                    profile!("store");

                    // We don't do this yet, but here the snapshot will be filtered differently for
                    // every player.
                    let mut sys = game::StoreSnapshotSys {
                        snapshot: game::WorldSnapshot::new(),
                        only_player: None,
                    };
                    sys.run_now(&self.game_state.world.res);
                    Some(sys.snapshot)
                } else {
                    None
                };

                let tick_data = tick::Data {
                    events: player_events.into_vec(),
                    snapshot: snapshot,
                    last_input_num: player.last_input_num,
                };

                player.tick_history.push_tick(self.next_tick, tick_data);

                if send_snapshot {
                    if rand::thread_rng().gen() {
                        // TMP: For testing delta encoding/decoding!
                        //continue;
                    }

                    let mut buffer = mem::replace(&mut self.write_buffer, Vec::new());
                    buffer.clear();

                    let mut writer = BitWriter::new(buffer);

                    {
                        profile!("write");
                        player.tick_history.delta_write_tick(
                            player.last_ack_tick,
                            self.next_tick,
                            &entity_classes,
                            &mut writer,
                        )?;
                    }

                    profile!("send");

                    let buffer = writer.into_inner()?;
                    host.send_game(player_id, &buffer)?;

                    mem::replace(&mut self.write_buffer, buffer);
                }
            }

            // 4.3. Advance counter
            self.next_tick += 1;
        }

        Ok(())
    }

    /// Send list of existing players to a new player in the next tick via events.
    fn send_player_list(&mut self, new_player: &mut Player) {
        // Only consider those players that are already registered in the game logic. The new player
        // will get information about other new players (that have joined but whose PlayerJoined
        // events have not been processed in a tick yet) with the regular shared events.
        let other_players = self.game_state.world.read_resource::<player::Players>();

        for (&other_id, other_player) in other_players.iter() {
            new_player.queued_events.push(player::JoinedEvent {
                id: other_id,
                info: other_player.info.clone(),
            });
        }
    }
}

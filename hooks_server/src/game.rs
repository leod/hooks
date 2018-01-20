use std::collections::BTreeMap;
use std::mem;

use rand::{self, Rng};

use bit_manager::BitWriter;

use shred::{Fetch, RunNow};

use common::{self, event, game, GameInfo, LeaveReason, PlayerId, PlayerInfo, TickDeltaNum, TickNum};
use common::net::protocol::ClientGameMsg;
use common::registry::Registry;
use common::repl::{player, tick};
use common::timer::{Stopwatch, Timer};

use host::{self, Host};

struct Player {
    join_tick: TickNum,
    last_ack_tick: Option<TickNum>,
    tick_history: tick::History<game::EntitySnapshot>,

    /// Events queued only for this player for the next tick. We currently use this to inform newly
    /// joined players of existing players, with a stack of `PlayerJoined` events.
    queued_events: event::Sink,
}

impl Player {
    pub fn new(join_tick: TickNum, event_reg: event::Registry) -> Player {
        Player {
            join_tick: join_tick,
            last_ack_tick: None,
            tick_history: tick::History::new(event_reg),
            queued_events: event::Sink::new(),
        }
    }
}

pub struct Game {
    state: game::State,
    players: BTreeMap<PlayerId, Player>,

    /// Timer to start the next tick.
    tick_timer: Timer,

    /// Number of tick we will start next.
    next_tick: TickNum,

    /// Stopwatch for advancing timers
    update_stopwatch: Stopwatch,

    /// Events queued for the next tick.
    queued_events: event::Sink,
}

fn register(game_info: &GameInfo, reg: &mut Registry) {
    common::auth::register(game_info, reg);
}

impl Game {
    pub fn new(game_info: GameInfo) -> Game {
        let mut state = {
            let mut reg = Registry::new();

            register(&game_info, &mut reg);

            game::State::from_registry(reg)
        };

        game::init::auth::create_state(&mut state.world);

        Game {
            state,
            players: BTreeMap::new(),
            tick_timer: Timer::new(game_info.tick_duration()),
            next_tick: 0,
            update_stopwatch: Stopwatch::new(),
            queued_events: event::Sink::new(),
        }
    }

    pub fn game_info(&self) -> Fetch<GameInfo> {
        self.state.world.read_resource::<GameInfo>()
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
                    let player_info = PlayerInfo::new(name.clone());

                    self.queued_events.push(player::JoinedEvent {
                        id: player_id,
                        info: player_info,
                    });

                    assert!(!self.players.contains_key(&player_id));

                    let mut player = Player::new(self.next_tick, self.state.event_reg.clone());
                    self.send_player_list(&mut player);

                    self.players.insert(player_id, player);
                }
                host::Event::PlayerLeft(player_id, reason) => {
                    self.queued_events.push(player::LeftEvent {
                        id: player_id,
                        reason,
                    });

                    assert!(self.players.contains_key(&player_id));
                    self.players.remove(&player_id);
                }
                host::Event::ClientGameMsg(player_id, msg) => {
                    assert!(self.players.contains_key(&player_id));

                    match msg {
                        ClientGameMsg::PlayerInput(input) => {
                            // TODO
                        }
                        ClientGameMsg::ReceivedTick(tick_num) => {
                            // Client has acknowledged a tick
                            let player = self.players.get_mut(&player_id).unwrap();

                            if tick_num >= self.next_tick {
                                // Invalid tick number! Forcefully disconnect the client.
                                // NOTE: The corresponding `host::Event::PlayerLeft` event will be
                                //       handled in the next iteration of the while loop.
                                host.force_disconnect(player_id, LeaveReason::InvalidMsg)?;
                            }

                            // Since game messages are unreliable, it is possible that we receive
                            // acknowledgements out of order
                            if tick_num > player.last_ack_tick.unwrap_or(0) {
                                player.last_ack_tick = Some(tick_num);

                                // Now we do not need snapshots from ticks older than that anymore.
                                // The server will always try to delta encode with respect to the
                                // last tick acknowledged by the client.
                                player.tick_history.prune_older_ticks(tick_num);
                            }
                        }
                    }
                }
            }
        }

        // 4. Run a tick periodically
        if self.tick_timer.trigger() {
            // 4.1. Run tick

            // Here, the state's `event::Sink` is empty. Push all the events that we have queued.
            self.state.push_events(self.queued_events.clear());

            let tick_events = self.state.run_tick_auth();

            // Can unwrap here, since replication errors should at most happen on the client-side
            let tick_events = tick_events.unwrap();

            // 4.2. Create snapshot for every player.
            for (_player_id, player) in self.players.iter_mut() {
                // We don't do this yet, but here the snapshot will be filtered differently for
                // every player.
                let mut sys = game::StoreSnapshotSys(game::WorldSnapshot::new());
                sys.run_now(&self.state.world.res);

                let snapshot = sys.0;

                // Events for this player are the special queued events as well as the shared
                // events of this tick
                let mut player_events = mem::replace(&mut player.queued_events, event::Sink::new());
                for event in &tick_events {
                    player_events.push_box(event.clone_event());
                }

                let tick_data = tick::Data {
                    events: player_events.into_vec(),
                    snapshot: Some(snapshot),
                };

                player.tick_history.push_tick(self.next_tick, tick_data);
            }

            // 4.3. Send delta snapshot to every player
            let entity_classes = self.state.world.read_resource::<game::EntityClasses>();

            for (&player_id, player) in self.players.iter_mut() {
                // TMP: For testing delta encoding/decoding!
                if rand::thread_rng().gen() {
                    //continue;
                }

                let mut writer = BitWriter::new(Vec::new());

                //println!("Sending tick {} with entities {:?}", self.next_tick, player.tick_history.get(self.next_tick).as_ref().unwrap().snapshot.as_ref().unwrap().0.keys());
                player.tick_history.delta_write_tick(
                    player.last_ack_tick,
                    self.next_tick,
                    &entity_classes,
                    &mut writer,
                )?;

                let data = writer.into_inner()?;

                host.send_game(player_id, &data)?;
            }

            // 4.4. Advance counter
            self.next_tick += 1;
        }

        Ok(())
    }

    /// Send list of existing players to a new player in the next tick via events.
    fn send_player_list(&mut self, new_player: &mut Player) {
        // Only consider those players that are already registered in the game logic. The new player
        // will get information about other new players (that have joined but whose PlayerJoined
        // events have not been processed in a tick yet) with the regular shared events.
        let other_players = self.state.world.read_resource::<player::Players>();

        for (&other_id, &(ref other_info, _entity)) in other_players.iter() {
            new_player.queued_events.push(player::JoinedEvent {
                id: other_id,
                info: other_info.clone(),
            });
        }
    }
}

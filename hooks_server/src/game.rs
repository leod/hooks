use std::collections::BTreeMap;
use std::mem;
use std::time::Instant;

use bit_manager::BitWriter;

use shred::{Fetch, RunNow};

use hooks_common::net::protocol::{ClientGameMsg, ServerCommMsg};
use hooks_common::net::transport::PeerId;
use hooks_common::registry::Registry;
use hooks_common::repl::{player, tick};
use hooks_common::INVALID_PLAYER_ID;
use hooks_common::{self, event, game, GameInfo, LeaveReason, PlayerId, PlayerInfo, PlayerInput,
                   TickDeltaNum, TickNum};
use hooks_util::profile;
use hooks_util::timer::{Stopwatch, Timer};

use bot::Bot;
use host::{self, Host};

#[derive(Clone, Debug)]
struct TimedInput {
    /// In which tick the client estimates we will run the input.
    target_tick: TickNum,

    /// The input to be executed.
    input: PlayerInput,

    /// When we received the input packet.
    receive_instant: Instant,
}

#[derive(Clone, Debug)]
struct LastRanInput {
    /// Tick in which the client executed the input.
    client_tick: TickNum,

    /// Tick in which the server executed the input.
    /// This will always be larger than `client_tick` due to clients living in the past.
    server_tick: TickNum,

    input: TimedInput,
}

struct Player {
    /// Unique in-game id. Note that this can be different from the `PeerId`, since bots can have
    /// player ids as well.
    id: PlayerId,

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
    queued_inputs: BTreeMap<TickNum, TimedInput>,

    /// Last input that has been executed from this client, if any.
    last_ran_input: Option<LastRanInput>,
}

impl Player {
    pub fn new(id: PeerId, join_tick: TickNum, event_reg: &event::Registry) -> Player {
        Player {
            id,
            join_tick,
            last_ack_tick: None,
            last_started_tick: None,
            tick_history: tick::History::new(event_reg.clone()),
            queued_events: event::Sink::new(),
            queued_inputs: BTreeMap::new(),
            last_ran_input: None,
        }
    }
}

pub struct Game {
    game_state: game::State,
    game_runner: game::run::AuthRunner,

    next_player_id: PlayerId,
    players: BTreeMap<PeerId, Player>,
    bots: Vec<(PlayerId, Bot)>,

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
    pub fn new(game_info: &GameInfo) -> Game {
        let mut game_state = {
            let mut reg = Registry::new();

            register(&mut reg, &game_info);

            game::State::from_registry(reg)
        };
        game::init::auth::create_state(&mut game_state.world);

        let game_runner = game::run::AuthRunner::new(&mut game_state.world);

        Game {
            game_state,
            game_runner,
            next_player_id: INVALID_PLAYER_ID + 1,
            players: BTreeMap::new(),
            bots: Vec::new(),
            tick_timer: Timer::new(game_info.tick_duration()),
            next_tick: 1,
            update_stopwatch: Stopwatch::new(),
            queued_events: event::Sink::new(),
            write_buffer: Vec::new(),
        }
    }

    pub fn game_info(&self) -> Fetch<GameInfo> {
        self.game_state.world.read_resource::<GameInfo>()
    }

    pub fn add_bot(&mut self, name: &str) -> PlayerId {
        let player_id = self.register_player(name);
        self.bots.push((player_id, Bot::default()));

        player_id
    }

    pub fn update(&mut self, host: &mut Host) -> Result<(), host::Error> {
        let update_duration = self.update_stopwatch.get_reset();

        // Detect players that are lagged too far behind
        for (&peer_id, player) in &self.players {
            let num_delta = if let Some(last_ack_tick) = player.last_ack_tick {
                assert!(last_ack_tick < self.next_tick);
                self.next_tick - last_ack_tick
            } else {
                // Player has not acknowledged a tick yet
                self.next_tick - player.join_tick + 1
            };

            if num_delta > TickNum::from(TickDeltaNum::max_value()) {
                // NOTE: In the future, if we have a higher tick rate, it might be better to send
                //       a full snapshot to players who are lagged too far behind to use delta
                //       encoding. Then, a different mechanism will need to be used to force
                //       disconnect lagged clients.
                info!(
                    "Player {}'s last acknowledged tick is {} ticks (ca. {:?}) in the past. \
                     Forcefully disconnecting.",
                    player.id,
                    num_delta,
                    self.tick_timer.period() * num_delta
                );
                host.force_disconnect(peer_id, LeaveReason::Lagged)?;
            }
        }

        // Handle network events and create resulting game events
        host.update(update_duration)?;

        while let Some(event) = host.service()? {
            self.handle_event(host, event)?;
        }

        // Run a tick periodically
        self.tick_timer += update_duration;
        if self.tick_timer.trigger() {
            self.start_tick(host)?;
        }

        Ok(())
    }

    /// Generate an id for a new player and notify the game logic about it in the next tick.
    fn register_player(&mut self, name: &str) -> PlayerId {
        let id = self.next_player_id;
        self.next_player_id += 1;

        let info = PlayerInfo::new(name.to_string());

        // At the start of the next tick, all players will receive an event that a new
        // player has joined. This induces the repl player management on server and
        // clients --- including the newly connected client.
        self.queued_events.push(player::JoinedEvent { id, info });

        id
    }

    fn handle_event(&mut self, host: &mut Host, event: host::Event) -> Result<(), host::Error> {
        match event {
            host::Event::PlayerJoined(peer_id, name) => {
                assert!(!self.players.contains_key(&peer_id));

                let player_id = self.register_player(&name);
                host.send_comm(
                    peer_id,
                    &ServerCommMsg::JoinGame {
                        your_player_id: player_id,
                    },
                )?;

                let mut player = Player::new(
                    player_id,
                    self.next_tick,
                    &self.game_state.event_reg.clone(),
                );

                // Send additional `JoinedEvent`s only for the new player, in the first tick
                // that it receives
                self.queue_player_list(&mut player);

                self.players.insert(peer_id, player);
            }
            host::Event::PlayerLeft(peer_id, reason) => {
                let player_id = self.players[&peer_id].id;

                // Inform game state on server and clients of player leaving
                self.queued_events.push(player::LeftEvent {
                    id: player_id,
                    reason,
                });

                self.players.remove(&peer_id);
            }
            host::Event::ClientGameMsg(peer_id, msg, receive_instant) => {
                assert!(self.players.contains_key(&peer_id));
                self.handle_client_game_msg(host, peer_id, msg, receive_instant)?;
            }
        }

        Ok(())
    }

    fn handle_client_game_msg(
        &mut self,
        host: &mut Host,
        peer_id: PeerId,
        msg: ClientGameMsg,
        receive_instant: Instant,
    ) -> Result<(), host::Error> {
        match msg {
            ClientGameMsg::ReceivedTick(tick_num) => {
                // Client has acknowledged a tick
                let player = self.players.get_mut(&peer_id).unwrap();

                if tick_num >= self.next_tick {
                    // Invalid tick number! Forcefully disconnect the client.
                    // NOTE: The corresponding `host::Event::PlayerLeft` event will be
                    //       handled in the next iteration of the while loop.
                    warn!(
                        "Player {} says he received tick {}, but we will start\
                         tick {} next, disconnecting",
                        player.id, tick_num, self.next_tick
                    );

                    return host.force_disconnect(peer_id, LeaveReason::InvalidMsg);
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
                            player.id, tick_num
                        );

                        return host.force_disconnect(player.id, LeaveReason::InvalidMsg);
                    }
                }
            }
            ClientGameMsg::StartedTick {
                tick: started_tick,
                target_tick,
                input,
            } => {
                let player = self.players.get_mut(&peer_id).unwrap();

                if started_tick >= self.next_tick {
                    // Got input for a tick that we haven't even started yet!
                    warn!(
                        "Player {} says he started tick {}, but we will start\
                         tick {} next, disconnecting",
                        player.id, started_tick, self.next_tick
                    );

                    return host.force_disconnect(peer_id, LeaveReason::InvalidMsg);
                }

                let is_new_tick: bool = match player.last_started_tick {
                    Some(last_started_tick) => {
                        if last_started_tick == started_tick {
                            // Player started tick twice!
                            warn!(
                                "Player {} started tick {} twice, disconnecting",
                                player.id, started_tick
                            );

                            return host.force_disconnect(peer_id, LeaveReason::InvalidMsg);
                        }

                        // Input might have been received out of order, ignore older
                        started_tick > last_started_tick
                    }
                    None => true,
                };

                if is_new_tick {
                    player.last_started_tick = Some(started_tick);
                }

                /*// If we don't receive a player's inputs, we fill by repeating the
                // previous input to get smoother behaviour. If the tick of that input
                // does arrive later, we should ignore it here.
                let is_new_input: bool = match &player.last_ran_input {
                    &Some(ref last_input) => started_tick > last_input.client_tick,
                    &None => true,
                };*/

                let is_new_input: bool = true;

                if is_new_input {
                    let timed_input = TimedInput {
                        target_tick,
                        input: input.clone(),
                        receive_instant,
                    };
                    player.queued_inputs.insert(started_tick, timed_input);
                }
            }
        }
        Ok(())
    }

    fn start_tick(&mut self, host: &mut Host) -> Result<(), host::Error> {
        profile!("tick");

        // Here, the state's `event::Sink` is empty. Push all the events that we have queued.
        assert!(
            self.game_state
                .world
                .read_resource::<event::Sink>()
                .is_empty()
        );
        self.game_state.push_events(self.queued_events.clear());

        // Collect every player's queued inputs whose time has come
        let game_info = self.game_info().clone();
        let next_tick = self.next_tick;

        let mut inputs = Vec::new();
        for (&peer_id, player) in &mut self.players {
            // TODO: Proper player input buffering
            let ping_secs = host.get_ping_secs(peer_id).unwrap();

            let player_inputs = player
                .queued_inputs
                .iter()
                .map(|(&client_tick, input)| {
                    // NOTE: This map should be monotonic in the tick
                    let target_tick = game_info.input_target_tick(ping_secs, client_tick);
                    (target_tick, client_tick, input.clone())
                })
                .filter(|&(target_tick, _, _)| target_tick <= next_tick)
                .collect::<Vec<_>>();

            if player_inputs.len() > 1 {
                debug!(
                    "player {}: input jump of {} (tick {})",
                    player.id,
                    player_inputs.len(),
                    next_tick,
                );
            }

            if let Some(&(_, client_tick, ref input)) = player_inputs.last() {
                if next_tick != input.target_tick {
                    debug!(
                        "run player {} input from tick {} in {} vs. client-estimated {}",
                        player.id, client_tick, next_tick, input.target_tick,
                    );
                }

                // Remember the last input we run
                player.last_ran_input = Some(LastRanInput {
                    client_tick,
                    server_tick: next_tick,
                    input: input.clone(),
                });

                for &(_, client_tick, _) in &player_inputs {
                    player.queued_inputs.remove(&client_tick).unwrap();
                }

                inputs.extend(player_inputs.iter().map(|&(target_tick, _, ref input)| {
                    (target_tick, (player.id, input.input.clone()))
                }));
            } else if let Some(last_ran_input) = player.last_ran_input.clone() {
                debug!(
                    "player {}: no input (tick {}), filling (from tick {}) \
                     with {} queued starting at {:?}",
                    player.id,
                    next_tick,
                    last_ran_input.server_tick,
                    player.queued_inputs.len(),
                    player.queued_inputs.iter().next().map(|input| *input.0),
                );

                player.last_ran_input = Some(LastRanInput {
                    /*// This would've been the client's next input.
                    client_tick: last_ran_input.client_tick + 1,*/
                    client_tick: last_ran_input.client_tick,
                    server_tick: next_tick,
                    input: last_ran_input.input.clone(),
                });

                // TODO: Which target tick to specify when filling input?
                inputs.push((next_tick, (player.id, last_ran_input.input.input.clone())));
            } else {
                debug!(
                    "player {}: no input (tick {}), can't fill",
                    player.id, next_tick,
                );
            }
        }

        inputs
            .sort_by(|&(target_tick_a, _), &(target_tick_b, _)| target_tick_a.cmp(&target_tick_b));

        let mut inputs = inputs
            .iter()
            .map(|&(_, ref player_input)| player_input.clone())
            .collect::<Vec<_>>();

        for &mut (player_id, ref mut bot) in self.bots.iter_mut() {
            inputs.push((player_id, bot.run()));
        }

        let tick_events = {
            profile!("run");
            self.game_runner.run_tick(&mut self.game_state, inputs)
        };

        // Can unwrap here, since replication errors should at most happen on the client-side
        let tick_events = tick_events.unwrap();

        // Record tick in history and send snapshots for every player
        profile!("tick history");

        let entity_classes = self.game_state.world.read_resource::<game::EntityClasses>();
        let send_snapshot = self.next_tick % self.game_info().ticks_per_snapshot == 0;

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

        for (&peer_id, player) in &mut self.players {
            // Events for this player are the special queued events as well as the shared
            // events of this tick
            let mut player_events = mem::replace(&mut player.queued_events, event::Sink::new());
            for event in &tick_events {
                player_events.push_box(event.clone_event());
            }

            {
                profile!("data");

                let tick_data = tick::Data {
                    events: player_events.into_vec(),
                    snapshot: snapshot.clone(),
                    last_input_tick: player
                        .last_ran_input
                        .as_ref()
                        .map(|input| input.client_tick),
                };
                player.tick_history.push_tick(self.next_tick, tick_data);
            }

            if send_snapshot {
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
                host.send_game(peer_id, buffer)?;

                //mem::replace(&mut self.write_buffer, buffer);
            }
        }

        self.next_tick += 1;

        Ok(())
    }

    /// Send list of existing players to a new player in the next tick via events.
    fn queue_player_list(&mut self, new_player: &mut Player) {
        // Only consider those players that are already registered in the game logic. The new player
        // will get information about other new players (that have joined but whose PlayerJoined
        // events have not been processed in a tick yet) with the regular shared events.
        let other_players = self.game_state.world.read_resource::<player::Players>();

        for (&other_player_id, other_player) in other_players.iter() {
            new_player.queued_events.push(player::JoinedEvent {
                id: other_player_id,
                info: other_player.info.clone(),
            });
        }
    }
}

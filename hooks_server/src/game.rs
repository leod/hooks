use std::collections::BTreeMap;
use std::mem;
use std::time;

use bit_manager::BitWriter;

use shred::RunNow;

use common::{self, event, game, GameInfo, LeaveReason, PlayerId, PlayerInfo, TickNum};
use common::net::protocol::ClientGameMsg;
use common::registry::Registry;
use common::repl::{player, tick};
use common::timer::{self, Timer};

use host::{self, Host};

pub const MAX_LAG_SECS: f64 = 5.0;

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

    /// Number of last started tick.
    last_tick: TickNum,

    /// Time that the last update call occured.
    last_update_instant: Option<time::Instant>,

    /// Events queued for the next tick.
    queued_events: event::Sink,
}

fn register(game_info: &GameInfo, reg: &mut Registry) {
    common::auth::register(game_info, reg);
}

impl Game {
    pub fn new(game_info: GameInfo) -> Game {
        let state = {
            let mut reg = Registry::new();

            register(&game_info, &mut reg);

            game::State::from_registry(reg)
        };

        Game {
            state,
            players: BTreeMap::new(),
            tick_timer: Timer::new(game_info.tick_duration()),
            last_tick: 0,
            last_update_instant: None,
            queued_events: event::Sink::new(),
        }
    }

    pub fn update(&mut self, host: &mut Host) -> Result<(), host::Error> {
        // 1. Advance timers
        if let Some(instant) = self.last_update_instant {
            // TODO: When no tick is run, iterations of `update` might be immeasurable, causing
            // timing errors.
            let duration = instant.elapsed();

            self.tick_timer += duration;
        }

        self.last_update_instant = Some(time::Instant::now());

        // 2. Detect players that are lagged too far behind
        for (&player_id, player) in self.players.iter() {
            let last_ack_tick = player.last_ack_tick.unwrap_or(player.join_tick);
            assert!(last_ack_tick <= self.last_tick);

            let tick_duration_sec = timer::duration_to_secs(self.tick_timer.period());
            let last_ack_elapsed = (self.last_tick - last_ack_tick) as f64 * tick_duration_sec;

            if last_ack_elapsed > MAX_LAG_SECS {
                info!(
                    "Player {}'s last acknowledged tick is {:.2} seconds in the past. \
                     Forcefully disconnecting.",
                    player_id, last_ack_elapsed
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

                    let mut player = Player::new(self.last_tick, self.state.event_reg.clone());
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

                            if tick_num > self.last_tick {
                                // Invalid tick number! Forcefully disconnect the client.
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
            self.last_tick += 1;

            // Here, the state's `event::Sink` is empty. Push all the events that we have queued.
            self.state.push_events(self.queued_events.clear());

            let tick_events = self.state.run_tick();

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
                    player_events.push_box((**event).clone());
                }

                let tick_data = tick::Data {
                    events: player_events.into_inner(),
                    snapshot: Some(snapshot),
                };

                player.tick_history.push_tick(self.last_tick, tick_data);
            }

            // 4.3. Send delta snapshot to every player
            let entity_classes = self.state.world.read_resource::<game::EntityClasses>();

            for (&player_id, player) in self.players.iter_mut() {
                let mut writer = BitWriter::new(Vec::new());

                player.tick_history.delta_write_tick(
                    player.last_ack_tick,
                    self.last_tick,
                    &entity_classes,
                    &mut writer,
                )?;

                let data = writer.into_inner()?;

                host.send_game(player_id, &data)?;
            }
        }

        Ok(())
    }

    /// Send list of existing players to a new player in the next tick via events.
    fn send_player_list(&mut self, new_player: &mut Player) {
        // Only consider those players that are already registered in the game logic. The new player
        // will get information about other new players (that have joined but whose PlayerJoined
        // events have not been processed in a tick yet) with the regular shared events.
        let other_players = self.state.world.read_resource::<player::Players>();

        for (&other_id, other_info) in other_players.iter() {
            new_player.queued_events.push(player::JoinedEvent {
                id: other_id,
                info: other_info.clone(),
            });
        }
    }
}

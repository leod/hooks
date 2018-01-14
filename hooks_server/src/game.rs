use std::collections::BTreeMap;
use std::time;

use common::{self, event, game, GameInfo, PlayerId, PlayerInfo, TickNum};
use common::net::protocol::ClientGameMsg;
use common::registry::Registry;
use common::repl::{player, tick};
use common::timer::Timer;

use host::{self, Host};

struct Player {
    last_ack_tick: Option<TickNum>,
    tick_history: tick::History<game::EntitySnapshot>,

    /// Events queued only for this player for the next tick. We currently use this to inform newly
    /// joined players of existing players, with a stack of `PlayerJoined` events.
    queued_events: event::Sink,
}

impl Player {
    pub fn new(event_reg: event::Registry) -> Player {
        Player {
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

    /// Time that the last update occured.
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

        // 2. Handle network events and create resulting game events
        while let Some(event) = host.service()? {
            match event {
                host::Event::PlayerJoined(player_id, name) => {
                    let player_info = PlayerInfo::new(name.clone());

                    self.queued_events.push(player::JoinedEvent {
                        id: player_id,
                        info: player_info,
                    });

                    assert!(!self.players.contains_key(&player_id));

                    let mut player = Player::new(self.state.event_reg.clone());
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
                    match msg {
                        ClientGameMsg::PlayerInput(input) => {
                            // TODO
                        }
                        ClientGameMsg::ReceivedTick(tick_num) => {
                            // Client has acknowledged a tick
                            let player = self.players.get_mut(&player_id).unwrap();

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

        // 3. Detect players that are lagged too far behind
        // TODO

        // 4. Run a tick periodically
        if self.tick_timer.trigger() {
            // Here, the state's `event::Sink` is empty. Push all the events that we have queued.
            self.state.push_events(self.queued_events.clear());

            // Run tick
            let events = self.state.run_tick();

            // Can unwrap here, since replication errors should at most happen on the client-side
            let events = events.unwrap();
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

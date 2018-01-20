use std::io::Cursor;
use std::time::Duration;

use rand::{self, Rng};

use shred::RunNow;
use specs::Join;

use bit_manager::BitReader;

use common::{self, event, game, GameInfo, PlayerId, TickNum};
use common::net::protocol::ClientGameMsg;
use common::registry::Registry;
use common::repl::{self, entity, tick};
use common::timer::{self, Timer};

use common::physics::Position; // TMP

use client::{self, Client};

#[derive(Debug)]
pub enum Error {
    Client(client::Error),
    Tick(tick::Error),
    Repl(repl::Error),
}

impl From<client::Error> for Error {
    fn from(error: client::Error) -> Error {
        Error::Client(error)
    }
}

impl From<tick::Error> for Error {
    fn from(error: tick::Error) -> Error {
        Error::Tick(error)
    }
}

impl From<repl::Error> for Error {
    fn from(error: repl::Error) -> Error {
        Error::Repl(error)
    }
}

pub struct Game {
    my_player_id: PlayerId,
    state: game::State,
    tick_history: tick::History<game::EntitySnapshot>,

    /// Timer to start the next tick.
    tick_timer: Timer,

    /// Number of last started tick.
    last_tick: Option<TickNum>,

    /// Newest tick of which we know that the server knows that we have received it.
    server_recv_ack_tick: Option<TickNum>,
}

fn register(game_info: &GameInfo, reg: &mut Registry) {
    common::view::register(game_info, reg);
}

#[derive(Debug)]
pub enum Event {
    Disconnected,
    TickStarted(Vec<Box<event::Event>>),
}

impl Game {
    pub fn new(my_player_id: PlayerId, game_info: &GameInfo) -> Game {
        let state = {
            let mut reg = Registry::new();

            register(game_info, &mut reg);

            game::State::from_registry(reg)
        };
        let tick_history = tick::History::new(state.event_reg.clone());

        Game {
            my_player_id,
            state,
            tick_history,
            tick_timer: Timer::new(game_info.tick_duration()),
            last_tick: None,
            server_recv_ack_tick: None,
        }
    }

    pub fn update(&mut self, client: &mut Client, delta: Duration) -> Result<Option<Event>, Error> {
        // Advance timers
        self.tick_timer += delta;

        // Handle network events
        while let Some(event) = client.service()? {
            match event {
                client::Event::Disconnected => {
                    return Ok(Some(Event::Disconnected));
                }
                client::Event::ServerGameMsg(data) => {
                    let mut reader = BitReader::new(Cursor::new(data));

                    let entity_classes = self.state.world.read_resource::<game::EntityClasses>();
                    let tick_nums = self.tick_history
                        .delta_read_tick(&entity_classes, &mut reader)?;

                    if let Some((old_tick_num, new_tick_num)) = tick_nums {
                        debug!("New tick {} w.r.t. {:?}", new_tick_num, old_tick_num);

                        if rand::thread_rng().gen() {
                            // TMP: For testing delta encoding/decoding!
                            let reply = ClientGameMsg::ReceivedTick(new_tick_num);
                            client.send_game(reply)?;
                        }

                        if let Some(old_tick_num) = old_tick_num {
                            // The fact that we have received a new delta encoded tick means that
                            // the server knows that we have the tick w.r.t. which it was encoded.
                            self.server_recv_ack_tick = Some(old_tick_num);
                        }
                    }
                }
            }
        }

        // Remove ticks from our history that:
        // 1. We know for sure will not be used by the server as a reference for delta encoding.
        // 2. We have already started.
        match (self.last_tick, self.server_recv_ack_tick) {
            (Some(last_tick), Some(server_recv_ack_tick)) => {
                if last_tick >= server_recv_ack_tick {
                    self.tick_history.prune_older_ticks(server_recv_ack_tick);
                }
            }
            _ => {}
        }

        // Start ticks
        if self.tick_timer.trigger() {
            let tick = if let Some(last_tick) = self.last_tick {
                let next_tick = last_tick + 1;
                self.tick_history.get(next_tick).map(|_| next_tick)
            } else {
                // Start our first tick
                self.tick_history.min_num()
            };

            if let Some(tick) = tick {
                self.last_tick = Some(tick);

                let tick_data = self.tick_history.get(tick).unwrap();

                //debug!("Starting tick {}", tick);

                if let &Some(ref snapshot) = &tick_data.snapshot {
                    //debug!("Loading snapshot");

                    entity::view::create_new_entities(&mut self.state.world, snapshot);

                    let mut sys = game::LoadSnapshotSys(snapshot);
                    sys.run_now(&self.state.world.res);
                }

                for pos in self.state.world.read::<Position>().join() {
                    //debug!("pos: {}", pos.pos);
                }

                let events = event::Sink::clone_from_vec(&tick_data.events);
                self.state.push_events(events.into_vec());
                self.state.run_tick()?;
            } else {
                //warn!("Waiting for tick...");
            }
        }

        Ok(None)
    }
}

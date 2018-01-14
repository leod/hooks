use std::io::Cursor;
use std::time;

use shred::RunNow;

use bit_manager::BitReader;

use common::{self, event, game, GameInfo, PlayerId, TickNum};
use common::net::protocol::ClientGameMsg;
use common::registry::Registry;
use common::repl::{self, entity, tick};
use common::timer::{self, Timer};

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

    /// Time that the last update call occured.
    last_update_instant: Option<time::Instant>,
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
            last_update_instant: None,
        }
    }

    pub fn update(&mut self, client: &mut Client) -> Result<Option<Event>, Error> {
        // Advance timers
        if let Some(instant) = self.last_update_instant {
            let duration = instant.elapsed();

            self.tick_timer += duration;
        }

        self.last_update_instant = Some(time::Instant::now());
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
                        debug!("New tick {} w.r.t. {}", new_tick_num, old_tick_num);

                        let reply = ClientGameMsg::ReceivedTick(new_tick_num);
                        client.send_game(reply)?;

                        // The fact that we have received a new tick means that the server knows
                        // that we have the tick w.r.t. which it was encoded, so we can remove
                        // older ticks from our history
                        self.tick_history.prune_older_ticks(old_tick_num);
                    }
                }
            }
        }

        // Start ticks
        if self.tick_timer.trigger() {
            let tick = 
                if let Some(last_tick) = self.last_tick {
                    let next_tick = last_tick + 1;
                    self.tick_history.get(next_tick).map(|_| next_tick)
                } else {
                    self.tick_history.min_num()
                };

            if let Some(tick) = tick {
                self.last_tick = Some(tick);

                let tick_data = self.tick_history.get(tick).unwrap();

                debug!("Starting tick {}", tick);

                let events = {
                    let mut events = Vec::new();
                    for event in &tick_data.events {
                        events.push((**event).clone());
                    }
                    events
                };

                if let &Some(ref snapshot) = &tick_data.snapshot {
                    debug!("Loading snapshot");

                    entity::view::create_new_entities(&mut self.state.world, snapshot); 

                    let mut sys = game::LoadSnapshotSys(snapshot);
                    sys.run_now(&self.state.world.res);
                }

                self.state.push_events(events);
                self.state.run_tick()?;
            } else {
                warn!("Waiting for tick...");
            }
        }

        Ok(None)
    }
}

use std::io::Cursor;

use bit_manager::BitReader;

use common::{self, event, game, GameInfo, PlayerId};
use common::net::protocol::ClientGameMsg;
use common::registry::Registry;
use common::repl::tick;

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
        }
    }

    pub fn update(&mut self, client: &mut Client) -> Result<Option<Event>, Error> {
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

        // For testing, start ticks immediately
        while let Some(min_num) = self.tick_history.min_num() {
            //debug!("Starting tick {}", min_num);

            // TODO
            break;
        }

        Ok(None)
    }
}

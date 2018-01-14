use common::{self, event, game, GameInfo, PlayerId};
use common::registry::Registry;

use client::{self, Client};

pub struct Game {
    my_player_id: PlayerId,
    state: game::State,
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

        Game {
            my_player_id,
            state,
        }
    }

    pub fn update(&mut self, client: &mut Client) -> Result<Option<Event>, client::Error> {
        while let Some(event) = client.service()? {
            match event {
                client::Event::Disconnected => {
                    return Ok(Some(Event::Disconnected));
                }
                client::Event::ServerGameMsg(data) => {
                    info!("Received game msg");
                }
            }
        }

        Ok(None)
    }
}

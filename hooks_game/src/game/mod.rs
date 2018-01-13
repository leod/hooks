use common::{self, event, game, GameInfo, PlayerId};
use common::event::Event;
use common::registry::Registry;

use client::{self, Client};

pub struct Game {
    my_player_id: PlayerId,
    state: game::State,
}

fn register(game_info: &GameInfo, reg: &mut Registry) {
    common::view::register(game_info, reg);
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

    pub fn update(&mut self, client: &mut Client) -> Result<Vec<Box<Event>>, client::Error> {
        while let Some(event) = client.service()? {}

        Ok(Vec::new())
    }
}

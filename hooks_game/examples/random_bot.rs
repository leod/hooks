extern crate hooks_common;
extern crate hooks_game;
extern crate hooks_util;
extern crate rand;

use std::thread;
use std::time::Duration;

use rand::Rng;

use hooks_common::PlayerInput;
use hooks_common::registry::Registry;

use hooks_game::client::Client;
use hooks_game::game::{self, Game};

use hooks_util::timer::{Stopwatch, Timer};

fn main() {
    let host = "localhost".to_string();
    let port = 32444;
    let name = "testy".to_string();

    let timeout_ms = 5000;

    let quit_prob = 0.1;
    let mut quit_timer = Timer::new(Duration::from_millis(200));

    loop {
        let mut client = Client::connect(&host, port, &name, timeout_ms).unwrap();

        let mut reg = Registry::new();
        hooks_game::game::register(&mut reg, client.game_info());

        let mut game = Game::new(reg, client.my_player_id(), client.game_info());
        client.ready().unwrap();

        let mut update_stopwatch = Stopwatch::new();
        let player_input = PlayerInput::default();

        loop {
            let update_duration = update_stopwatch.get_reset();
            quit_timer += update_duration;

            if quit_timer.trigger() && rand::thread_rng().gen::<f64>() <= quit_prob {
                break;
            }

            match game.update(&mut client, &player_input, update_duration)
                .unwrap()
            {
                Some(game::Event::Disconnected) => {
                    return;
                }
                _ => {}
            }

            // TODO: Send random input

            thread::sleep(Duration::from_millis(1));
        }
    }
}

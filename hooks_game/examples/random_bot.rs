extern crate rand;
extern crate hooks_game; 
extern crate hooks_common;

use std::thread;
use std::time::Duration;

use rand::Rng;

use hooks_game::client::Client;
use hooks_game::game::{self, Game};

use hooks_common::timer::{Timer, Stopwatch};

fn main() {
    let host = "localhost".to_string();
    let port = 32444;
    let name = "testy".to_string();

    let timeout_ms = 5000;

    let quit_prob = 0.1;
    let mut quit_timer = Timer::new(Duration::from_millis(200));

    loop {
        let mut client = Client::connect(&host, port, &name, timeout_ms).unwrap();
        let mut game = Game::new(client.my_player_id(), client.game_info());
        client.ready().unwrap();

        let mut update_stopwatch = Stopwatch::new();

        loop {
            {
                let duration = update_stopwatch.get_reset();
                quit_timer += duration;
            }

            if quit_timer.trigger() && rand::thread_rng().gen::<f64>() <= quit_prob {
                break;
            }

            match game.update(&mut client).unwrap() {
                Some(game::Event::Disconnected) => {
                    return;
                }
                _ => {}
            }

            // TODO: Send random input

            thread::yield_now();
        }
    }
}

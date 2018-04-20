extern crate hooks_client;
extern crate hooks_game;
extern crate hooks_util;
extern crate rand;

use std::thread;
use std::time::Duration;

use rand::Rng;

use hooks_game::registry::Registry;
use hooks_game::PlayerInput;

use hooks_client::client::Client;
use hooks_client::game::{self, Game};

use hooks_util::timer::{Stopwatch, Timer};

fn main() {
    let host = "localhost".to_string();
    let port = 32444;
    let name = "testy".to_string();

    let timeout_ms = 5000;

    let shoot_prob = 0.01;
    let move_prob = 0.95;
    let rot_speed = 0.1;

    let mut quit_timer = Timer::new(Duration::from_secs(100000));

    loop {
        let mut client = Client::connect(&host, port, &name, timeout_ms).unwrap();
        let my_player_id = client.ready(timeout_ms).unwrap();

        let mut reg = Registry::new();
        hooks_client::game::register(&mut reg, client.game_info());

        let mut game = Game::new(reg, my_player_id, client.game_info(), false);

        let mut update_stopwatch = Stopwatch::new();
        let mut player_input = PlayerInput::default();
        let mut rng = rand::thread_rng();

        loop {
            let update_duration = update_stopwatch.get_reset();
            quit_timer += update_duration;

            let quit_prob = 1.0 - (-quit_timer.progress() as f64).exp();
            if rng.gen::<f64>() < quit_prob {
                break;
            }

            if rng.gen::<f64>() <= shoot_prob {
                player_input.shoot_one = !player_input.shoot_one;
            }
            if rng.gen::<f64>() <= shoot_prob {
                player_input.shoot_two = !player_input.shoot_two;
            }
            player_input.move_forward = rng.gen::<f64>() <= move_prob;
            if rng.gen::<bool>() {
                player_input.rot_angle += rng.gen::<f32>() * rot_speed;
            } else {
                player_input.rot_angle -= rng.gen::<f32>() * rot_speed;
            }

            match game.update(&mut client, &player_input, update_duration)
                .unwrap()
            {
                Some(game::Event::Disconnected) => {
                    return;
                }
                _ => {}
            }

            thread::sleep(Duration::from_millis(1));
        }
    }
}

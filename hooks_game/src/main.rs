extern crate bit_manager;
extern crate env_logger;
extern crate ggez;
extern crate hooks_common as common;
#[macro_use]
extern crate log;

mod client;
mod game;

use std::thread;

use client::Client;
use game::Game;

struct Config {
    host: String,
    port: u16,
    name: String,
}

struct Main {}

fn main() {
    env_logger::init();

    let config = Config {
        host: "localhost".to_string(),
        port: 32444,
        name: "testy".to_string(),
    };
    let timeout_ms = 5000;

    let mut client = Client::connect(&config.host, config.port, &config.name, timeout_ms).unwrap();
    info!(
        "Connected to {}:{} with player id {} and game info {:?}",
        config.host,
        config.port,
        client.my_player_id(),
        client.game_info()
    );

    let mut game = Game::new(client.my_player_id(), client.game_info());
    client.ready().unwrap();

    loop {
        match game.update(&mut client).unwrap() {
            Some(game::Event::Disconnected) => {
                info!("Got disconnected! Bye.");
                return;
            }
            Some(game::Event::TickStarted(events)) => {}
            None => {}
        }

        thread::yield_now();
    }
}

extern crate bit_manager;
extern crate hooks_common as common;
#[macro_use]
extern crate log;
extern crate env_logger;

mod client;

use client::Client;

struct Config {
    host: String,
    port: u16,
    name: String,
}

fn main() {
    env_logger::init();

    let config = Config {
        host: "localhost".to_string(),
        port: 32444,
        name: "testy".to_string(),
    };
    let timeout_ms = 5000;

    let mut client = Client::connect(&config.host, config.port, &config.name, timeout_ms).unwrap();

    info!("Connected to {}:{}", config.host, config.port);
}

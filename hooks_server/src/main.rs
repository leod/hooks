#![feature(entry_or_default)]

extern crate bit_manager;
extern crate env_logger;
extern crate hooks_game;
#[cfg(feature = "show")]
extern crate hooks_show;
#[macro_use]
extern crate hooks_util;
#[macro_use]
extern crate log;
extern crate rand;
extern crate shred;

mod bot;
mod game;
mod host;
mod server;

use hooks_game::{GameInfo, MapInfo};

use server::Server;

fn main() {
    env_logger::init();

    let map_info = MapInfo;
    let game_info = GameInfo {
        ticks_per_second: 60,
        ticks_per_snapshot: 1,
        map_info,
        player_entity_class: "player".to_string(),
        server_target_lag_inputs: 1,
        client_target_lag_snapshots: 2,
    };
    let config = server::Config {
        port: 32444,
        game_info,
        num_bots: 5,
    };

    let mut server = Server::create(&config).unwrap();
    server.run().unwrap();
}

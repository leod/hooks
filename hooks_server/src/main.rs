extern crate bit_manager;
extern crate env_logger;
extern crate hooks_common as common;
#[macro_use]
extern crate log;

mod client;
mod host;
mod game;
mod server;

use common::{GameInfo, MapInfo};

use server::Server;

fn main() {
    env_logger::init();

    let map_info = MapInfo;
    let game_info = GameInfo {
        ticks_per_second: 20,
        map_info,
    };
    let config = server::Config {
        port: 32444,
        game_info,
    };

    let mut server = Server::create(config).unwrap();
    server.run().unwrap();
}

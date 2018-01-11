use common::GameInfo;
use common::net::protocol::{ClientCommMsg, ServerCommMsg};
use common::net::transport;

use client::{self, Client};

struct Host {
    host: transport::Host,
    game_info: GameInfo,
    clients: Vec<Client>,
}

impl Host {
    pub fn service(&mut self) {
        while let Some(event) = self.host.service(0) {
            match event {
                transport::Event::Connect(peer) => {}
                transport::Event::Receive(peer, channel, packet) => {}
                transport::Event::Disconnect(peer) => {}
            }
        }
    }
}

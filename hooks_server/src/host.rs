use std::collections::{BTreeMap, BTreeSet};

use common::{GameInfo, PlayerId};
use common::net::protocol::{ClientCommMsg, ServerCommMsg};
use common::net::transport;

use client::{self, Client};

struct Host {
    host: transport::Host,
    game_info: GameInfo,
    next_player_id: PlayerId,
    clients: BTreeMap<PlayerId, Client>,
}

enum Event {
    PlayerConnected(PlayerId, String),
    PlayerDisconnected(PlayerId),
}

impl Host {
    pub fn new(host: transport::Host, game_info: GameInfo) -> Host {
        Host {
            host,
            game_info,
            next_player_id: 0,
            clients: BTreeMap::new(),
        }
    }

    pub fn service(&mut self) -> Result<Option<Event>, transport::Error> {
        if let Some(event) = self.host.service(0)? {
            Ok(match event {
                transport::Event::Connect(peer) => {
                    let id = self.next_player_id;
                    self.next_player_id += 1;

                    peer.set_data(id as usize);

                    None
                }
                transport::Event::Receive(peer, channel, packet) => None,
                transport::Event::Disconnect(peer) => None,
            })
        } else {
            Ok(None)
        }
    }
}

use std::collections::BTreeMap;
use std::collections::btree_map;

use bit_manager::{self, BitRead, BitReader, BitWrite, BitWriter};

use common::{GameInfo, LeaveReason, PlayerId, INVALID_PLAYER_ID};
use common::net::protocol::{ClientCommMsg, ClientGameMsg, ServerCommMsg, CHANNEL_COMM,
                            CHANNEL_GAME, NUM_CHANNELS};
use common::net::transport;

use client::{self, Client};

#[derive(Debug)]
pub enum Error {
    InvalidChannel,
    ConnectedTwice,
    NotConnected,
    InvalidReady,
    Transport(transport::Error),
    BitManager(bit_manager::Error),
}

impl From<transport::Error> for Error {
    fn from(error: transport::Error) -> Error {
        Error::Transport(error)
    }
}

impl From<bit_manager::Error> for Error {
    fn from(error: bit_manager::Error) -> Error {
        Error::BitManager(error)
    }
}

pub struct Host {
    host: transport::Host,
    game_info: GameInfo,
    next_player_id: PlayerId,
    clients: BTreeMap<PlayerId, Client>,
}

pub enum Event {
    PlayerJoined(PlayerId, String),
    PlayerLeft(PlayerId, LeaveReason),
    ClientGameMsg(PlayerId, ClientGameMsg),
}

// TODO
pub const PEER_COUNT: usize = 32;

impl Host {
    pub fn create(port: u16, game_info: GameInfo) -> Result<Host, transport::Error> {
        let host = transport::Host::create_server(port, PEER_COUNT, NUM_CHANNELS, 0, 0)?;

        Ok(Host {
            host,
            game_info,
            next_player_id: INVALID_PLAYER_ID + 1,
            clients: BTreeMap::new(),
        })
    }

    fn disconnect(&mut self, peer: &transport::Peer, reason: LeaveReason) -> Option<Event> {
        let player_id = peer.data() as PlayerId;

        // This allows us to identify that the peer has been disconnected
        peer.set_data(INVALID_PLAYER_ID as usize);

        if let btree_map::Entry::Occupied(entry) = self.clients.entry(player_id) {
            // We have accepted this client. Does the game logic know of it?
            let ingame = entry.get().ingame();

            entry.remove();

            if ingame {
                // The player is already known to the game logic
                Some(Event::PlayerLeft(player_id, reason))
            } else {
                None
            }
        } else {
            // Player is unknown to game logic
            info!("No event for disconnect from player {}", player_id);

            None
        }
    }

    pub fn service(&mut self) -> Result<Option<Event>, Error> {
        if let Some(event) = self.host.service(0)? {
            match event {
                transport::Event::Connect(peer) => {
                    let player_id = self.next_player_id;
                    self.next_player_id += 1;

                    assert!(player_id != INVALID_PLAYER_ID);
                    assert!(!self.clients.contains_key(&player_id));

                    peer.set_data(player_id as usize);

                    Ok(None)
                }
                transport::Event::Receive(peer, channel, packet) => {
                    let player_id = peer.data() as PlayerId;

                    if player_id == INVALID_PLAYER_ID {
                        info!("Player {} is disconnected, ignoring packet", player_id);
                        Ok(None)
                    } else {
                        match self.handle_receive(&peer, channel, packet) {
                            Ok(event) => Ok(event),
                            Err(error) => {
                                warn!(
                                    "Error while handling received packet from player {}: {:?}.\
                                     Disconnecting.",
                                    player_id, error
                                );

                                peer.disconnect(666);

                                Ok(self.disconnect(&peer, LeaveReason::InvalidMsg))
                            }
                        }
                    }
                }
                transport::Event::Disconnect(peer) => {
                    Ok(self.disconnect(&peer, LeaveReason::Disconnected))
                }
            }
        } else {
            // No transport event
            Ok(None)
        }
    }

    fn send_comm(&self, receiver_id: PlayerId, msg: ServerCommMsg) -> Result<(), Error> {
        let data = {
            let mut writer = BitWriter::new(Vec::new());
            writer.write(&msg)?;
            writer.into_inner()?
        };

        let packet = transport::Packet::create(&data, transport::PacketFlag::Reliable)?;

        self.clients[&receiver_id]
            .peer
            .send(CHANNEL_COMM, packet)
            .map_err(|error| Error::Transport(error))
    }

    fn handle_receive(
        &mut self,
        peer: &transport::Peer,
        channel: u8,
        packet: transport::ReceivedPacket,
    ) -> Result<Option<Event>, Error> {
        let player_id = peer.data() as PlayerId;
        assert!(player_id != INVALID_PLAYER_ID);

        if channel == CHANNEL_COMM {
            // Communication messages are handled here
            let msg = {
                let mut reader = BitReader::new(packet.data());
                reader.read::<ClientCommMsg>()?
            };

            match msg {
                ClientCommMsg::WishConnect { name } => {
                    if !self.clients.contains_key(&player_id) {
                        info!(
                            "Player {} with name {} wishes to connect, accepting",
                            player_id, name
                        );

                        // Ok, first connection wish
                        self.clients
                            .insert(player_id, Client::new(peer.clone(), name.clone()));

                        // Inform the client
                        let reply = ServerCommMsg::AcceptConnect {
                            your_id: player_id,
                            game_info: self.game_info.clone(),
                        };
                        self.send_comm(player_id, reply)?;

                        Ok(None)
                    } else {
                        Err(Error::ConnectedTwice)
                    }
                }
                ClientCommMsg::Ready => {
                    if let Some(client) = self.clients.get_mut(&player_id) {
                        if client.state == client::State::Connected {
                            client.state = client::State::Ready;
                            Ok(Some(Event::PlayerJoined(player_id, client.name.clone())))
                        } else {
                            Err(Error::InvalidReady)
                        }
                    } else {
                        Err(Error::NotConnected)
                    }
                }
            }
        } else if channel == CHANNEL_GAME {
            // Game messages are relayed as events
            match self.clients.get(&player_id) {
                Some(client) => {
                    if client.ingame() {
                        let msg = {
                            let mut reader = BitReader::new(packet.data());
                            reader.read::<ClientGameMsg>()?
                        };

                        Ok(Some(Event::ClientGameMsg(player_id, msg)))
                    } else {
                        // Just discard game messages from players who are not ready
                        // (i.e. ingame) yet
                        Ok(None)
                    }
                }
                None => Ok(None),
            }
        } else {
            Err(Error::InvalidChannel)
        }
    }
}

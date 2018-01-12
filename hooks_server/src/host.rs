use std::collections::BTreeMap;

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
    PlayerConnected(PlayerId, String),
    PlayerDisconnected(PlayerId, LeaveReason),
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

                    match self.handle_receive(&peer, channel, packet) {
                        Ok(event) => Ok(event),
                        Err(error) => {
                            warn!(
                                "Error while handling received packet from player {}: {:?}.\
                                 Disconnecting.",
                                player_id, error
                            );

                            // The client will be removed from our client list by the enet
                            // disconnect event.
                            peer.disconnect(666);
                            self.clients.get_mut(&player_id).unwrap().state =
                                client::State::Disconnected;

                            // However, the game logic on all clients is immediately notified of
                            // the disconnection.
                            Ok(Some(Event::PlayerDisconnected(
                                player_id,
                                LeaveReason::InvalidMsg,
                            )))
                        }
                    }
                }
                transport::Event::Disconnect(peer) => {
                    let player_id = peer.data() as PlayerId;

                    assert!(self.clients.contains_key(&player_id));

                    let state = self.clients.remove(&player_id).unwrap().state;

                    if state != client::State::Disconnected {
                        // Game logic has not been notified of disconnection yet
                        Ok(Some(Event::PlayerDisconnected(
                            player_id,
                            LeaveReason::Disconnected,
                        )))
                    } else {
                        // This player has been forcefully disconnected by us
                        Ok(None)
                    }
                }
            }
        } else {
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

        if let Some(&client::State::Disconnected) = self.clients.get(&player_id).map(|client| &client.state) {
            info!(
                "Received packet from player {}, whom we disconnected. Ignoring.",
                player_id
            );
            return Ok(None);
        }

        if channel == CHANNEL_COMM {
            // Communication messages are handled here
            let msg = {
                let mut reader = BitReader::new(packet.data());
                reader.read::<ClientCommMsg>()?
            };

            match msg {
                ClientCommMsg::WishConnect { name } => {
                    if !self.clients.contains_key(&player_id) {
                        // Ok, first connection wish
                        self.clients.insert(player_id, Client::new(peer.clone()));

                        // Inform the client
                        let reply = ServerCommMsg::AcceptConnect {
                            your_id: player_id,
                            game_info: self.game_info.clone(),
                        };
                        self.send_comm(player_id, reply)?;

                        Ok(Some(Event::PlayerConnected(player_id, name)))
                    } else {
                        Err(Error::ConnectedTwice)
                    }
                }
            }
        } else if channel == CHANNEL_GAME {
            let msg = {
                let mut reader = BitReader::new(packet.data());
                reader.read::<ClientGameMsg>()?
            };

            Ok(Some(Event::ClientGameMsg(player_id, msg)))
        } else {
            Err(Error::InvalidChannel)
        }
    }
}

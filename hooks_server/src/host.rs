use std::collections::{btree_map, BTreeMap, VecDeque};
use std::time::Duration;

use bit_manager::{self, BitRead, BitReader, BitWrite, BitWriter};

use hooks_common::net::protocol::{self, ClientCommMsg, ClientGameMsg, ServerCommMsg, CHANNEL_COMM,
                                  CHANNEL_GAME, CHANNEL_TIME, NUM_CHANNELS};
use hooks_common::net::{self, transport};
use hooks_common::{GameInfo, LeaveReason, PlayerId, INVALID_PLAYER_ID};

use client::{self, Client};

#[derive(Debug)]
pub enum Error {
    InvalidChannel,
    ConnectedTwice,
    NotConnected,
    InvalidReady,
    Time(net::time::Error),
    Transport(transport::Error),
    BitManager(bit_manager::Error),
}

impl From<net::time::Error> for Error {
    fn from(error: net::time::Error) -> Error {
        Error::Time(error)
    }
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
    queued_events: VecDeque<Event>,
}

#[derive(Debug, Clone)]
pub enum Event {
    PlayerJoined(PlayerId, String),
    PlayerLeft(PlayerId, LeaveReason),
    ClientGameMsg(PlayerId, ClientGameMsg),
}

// TODO: Transport peer count?
pub const PEER_COUNT: usize = 64;

impl Host {
    pub fn create(port: u16, game_info: GameInfo) -> Result<Host, transport::Error> {
        let host = transport::Host::create_server(port, PEER_COUNT, NUM_CHANNELS, 0, 0)?;

        Ok(Host {
            host,
            game_info,
            next_player_id: INVALID_PLAYER_ID + 1,
            clients: BTreeMap::new(),
            queued_events: VecDeque::new(),
        })
    }

    fn on_disconnect(&mut self, peer: &transport::Peer, reason: LeaveReason) -> Option<Event> {
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

    pub fn force_disconnect(
        &mut self,
        player_id: PlayerId,
        reason: LeaveReason,
    ) -> Result<(), Error> {
        let peer = self.clients[&player_id].peer.clone();

        peer.disconnect(protocol::leave_reason_to_u32(reason));

        if let Some(event) = self.on_disconnect(&peer, reason) {
            // Let game logic handle this `PlayerLeft` event with the next `service` calls
            self.queued_events.push_back(event.clone());
        }

        Ok(())
    }

    pub fn update(&mut self, delta: Duration) -> Result<(), Error> {
        for client in self.clients.values_mut() {
            client.net_time.update(&client.peer, delta)?;
        }
        Ok(())
    }

    pub fn service(&mut self) -> Result<Option<Event>, Error> {
        // If we have some queued events, use them first. Currently, these are queued only if a
        // player has been forcefully disconnected.
        if let Some(event) = self.queued_events.pop_front() {
            return Ok(Some(event));
        }

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

                                peer.disconnect(protocol::leave_reason_to_u32(
                                    LeaveReason::InvalidMsg,
                                ));

                                Ok(self.on_disconnect(&peer, LeaveReason::InvalidMsg))
                            }
                        }
                    }
                }
                transport::Event::Disconnect(peer) => {
                    Ok(self.on_disconnect(&peer, LeaveReason::Disconnected))
                }
            }
        } else {
            // No transport event
            Ok(None)
        }
    }

    pub fn flush(&self) -> Result<(), Error> {
        self.host.flush();
        Ok(())
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

    pub fn send_game(&self, receiver_id: PlayerId, data: &[u8]) -> Result<(), Error> {
        assert!(self.clients[&receiver_id].ingame());

        let packet = transport::Packet::create(data, transport::PacketFlag::Unsequenced)?;

        self.clients[&receiver_id]
            .peer
            .send(CHANNEL_GAME, packet)
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
        } else if channel == CHANNEL_TIME {
            if let Some(client) = self.clients.get_mut(&player_id) {
                client.net_time.receive(&client.peer, packet)?;
            }
            Ok(None)
        } else if channel == CHANNEL_GAME {
            // Game messages are relayed as events
            if let Some(client) = self.clients.get(&player_id) {
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
            } else {
                // We have not accepted this player's connection wish yet, ignore game messages
                Ok(None)
            }
        } else {
            Err(Error::InvalidChannel)
        }
    }
}

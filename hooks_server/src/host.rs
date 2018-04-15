use std::collections::{btree_map, BTreeMap, VecDeque};
use std::time::Duration;

use bit_manager::{self, BitRead, BitReader, BitWrite, BitWriter};

use hooks_common::net;
use hooks_common::net::protocol::{self, ClientCommMsg, ClientGameMsg, ServerCommMsg, CHANNEL_COMM,
                                  CHANNEL_GAME, CHANNEL_TIME, NUM_CHANNELS};
use hooks_common::net::transport::{self, async, enet, ChannelId, Host as _Host, Packet,
                                   PacketFlag, PeerId};
use hooks_common::{GameInfo, LeaveReason, PlayerId, INVALID_PLAYER_ID};

type MyHost = async::Host<enet::Host, ()>;

#[derive(Debug)]
pub enum Error {
    InvalidChannel,
    ConnectedTwice,
    NotConnected,
    InvalidReady,
    Time(net::time::Error<<MyHost as transport::Host>::Error>),
    EnetTransport(enet::Error),
    AsyncTransport(async::Error),
    BitManager(bit_manager::Error),
}

impl From<net::time::Error<<MyHost as transport::Host>::Error>> for Error {
    fn from(error: net::time::Error<<MyHost as transport::Host>::Error>) -> Error {
        Error::Time(error)
    }
}

impl From<enet::Error> for Error {
    fn from(error: enet::Error) -> Error {
        Error::EnetTransport(error)
    }
}

impl From<async::Error> for Error {
    fn from(error: async::Error) -> Error {
        Error::AsyncTransport(error)
    }
}

impl From<bit_manager::Error> for Error {
    fn from(error: bit_manager::Error) -> Error {
        Error::BitManager(error)
    }
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum ClientState {
    /// We have acknowledged the connection and sent game info.
    Connected,

    /// The client has received the game info and is ready to receive ticks.
    Ready,
}

pub struct Client {
    pub name: String,
    pub state: ClientState,
    pub net_time: net::time::Time,
}

impl Client {
    pub fn new(name: String) -> Client {
        Client {
            name,
            state: ClientState::Connected,
            net_time: net::time::Time::new(),
        }
    }

    pub fn ingame(&self) -> bool {
        match self.state {
            ClientState::Connected => false,
            ClientState::Ready => true,
        }
    }
}

pub struct Host {
    host: MyHost,
    game_info: GameInfo,
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
    pub fn create(port: u16, game_info: GameInfo) -> Result<Host, Error> {
        let host = enet::Host::create_server(port, PEER_COUNT, NUM_CHANNELS, 0, 0)?;
        let host = async::Host::spawn(host);

        Ok(Host {
            host,
            game_info,
            clients: BTreeMap::new(),
            queued_events: VecDeque::new(),
        })
    }

    fn on_disconnect(&mut self, id: PlayerId, reason: LeaveReason) -> Option<Event> {
        if let btree_map::Entry::Occupied(entry) = self.clients.entry(id) {
            debug!("Disconnecting peer {}", id);

            // We have accepted this client. Does the game logic know of it?
            let ingame = entry.get().ingame();

            entry.remove();

            if ingame {
                // The player is already known to the game logic
                Some(Event::PlayerLeft(id, reason))
            } else {
                None
            }
        } else {
            // Player is not fully connected
            info!("No event for disconnect from player {}", id);

            None
        }
    }

    pub fn force_disconnect(
        &mut self,
        player_id: PlayerId,
        reason: LeaveReason,
    ) -> Result<(), Error> {
        self.host
            .disconnect(player_id, protocol::leave_reason_to_u32(reason))?;

        if let Some(event) = self.on_disconnect(player_id, reason) {
            // Let game logic handle this `PlayerLeft` event with the next `service` calls
            self.queued_events.push_back(event.clone());
        }

        Ok(())
    }

    pub fn update(&mut self, delta: Duration) -> Result<(), Error> {
        for (&peer_id, client) in self.clients.iter_mut() {
            client.net_time.update(&mut self.host, peer_id, delta)?;
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
                transport::Event::Connect(peer_id) => {
                    assert!(peer_id != INVALID_PLAYER_ID);
                    assert!(!self.clients.contains_key(&peer_id));

                    Ok(None)
                }
                transport::Event::Receive(peer_id, channel, packet) => {
                    assert!(peer_id != INVALID_PLAYER_ID);

                    match self.handle_receive(peer_id, channel, packet.data()) {
                        Ok(event) => Ok(event),
                        Err(error) => {
                            warn!(
                                "Error while handling received packet from player {}: {:?}.\
                                 Disconnecting.",
                                peer_id, error
                            );

                            self.host.disconnect(
                                peer_id,
                                protocol::leave_reason_to_u32(LeaveReason::InvalidMsg),
                            )?;

                            Ok(self.on_disconnect(peer_id, LeaveReason::InvalidMsg))
                        }
                    }
                }
                transport::Event::Disconnect(peer_id) => {
                    Ok(self.on_disconnect(peer_id, LeaveReason::Disconnected))
                }
            }
        } else {
            // No transport event
            Ok(None)
        }
    }

    pub fn flush(&mut self) -> Result<(), Error> {
        self.host.flush()?;
        Ok(())
    }

    fn send_comm(&mut self, receiver_id: PlayerId, msg: ServerCommMsg) -> Result<(), Error> {
        let data = {
            let mut writer = BitWriter::new(Vec::new());
            writer.write(&msg)?;
            writer.into_inner()?
        };

        Ok(self.host
            .send(receiver_id, CHANNEL_COMM, PacketFlag::Reliable, &data)?)
    }

    pub fn send_game(&mut self, receiver_id: PlayerId, data: &[u8]) -> Result<(), Error> {
        assert!(self.clients[&receiver_id].ingame());

        Ok(self.host
            .send(receiver_id, CHANNEL_GAME, PacketFlag::Unsequenced, data)?)
    }

    fn handle_receive(
        &mut self,
        peer_id: PeerId,
        channel: ChannelId,
        data: &[u8],
    ) -> Result<Option<Event>, Error> {
        assert!(peer_id != INVALID_PLAYER_ID);

        if channel == CHANNEL_COMM {
            // Communication messages are handled here
            let msg = {
                let mut reader = BitReader::new(data);
                reader.read::<ClientCommMsg>()?
            };

            match msg {
                ClientCommMsg::WishConnect { name } => {
                    if !self.clients.contains_key(&peer_id) {
                        debug!(
                            "Player {} with name {} wishes to connect, accepting",
                            peer_id, name
                        );

                        // Ok, first connection wish
                        self.clients.insert(peer_id, Client::new(name.clone()));

                        // Inform the client
                        let reply = ServerCommMsg::AcceptConnect {
                            your_id: peer_id,
                            game_info: self.game_info.clone(),
                        };
                        self.send_comm(peer_id, reply)?;

                        Ok(None)
                    } else {
                        Err(Error::ConnectedTwice)
                    }
                }
                ClientCommMsg::Ready => {
                    if let Some(client) = self.clients.get_mut(&peer_id) {
                        if client.state == ClientState::Connected {
                            client.state = ClientState::Ready;
                            Ok(Some(Event::PlayerJoined(peer_id, client.name.clone())))
                        } else {
                            Err(Error::InvalidReady)
                        }
                    } else {
                        Err(Error::NotConnected)
                    }
                }
            }
        } else if channel == CHANNEL_TIME {
            if let Some(client) = self.clients.get_mut(&peer_id) {
                client.net_time.receive(&mut self.host, peer_id, data)?;
            }
            Ok(None)
        } else if channel == CHANNEL_GAME {
            // Game messages are relayed as events
            if let Some(client) = self.clients.get(&peer_id) {
                if client.ingame() {
                    let msg = {
                        let mut reader = BitReader::new(data);
                        reader.read::<ClientGameMsg>()?
                    };

                    Ok(Some(Event::ClientGameMsg(peer_id, msg)))
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

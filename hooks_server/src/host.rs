use std::collections::{btree_map, BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use bit_manager::{self, BitRead, BitReader, BitWrite, BitWriter};

use hooks_game::net;
use hooks_game::net::protocol::{self, ClientCommMsg, ClientGameMsg, ServerCommMsg, CHANNEL_COMM,
                                CHANNEL_GAME, NUM_CHANNELS};
use hooks_game::net::transport::{self, async, enet, lag_loss};
use hooks_game::net::transport::{ChannelId, Host as _Host, Packet, PacketFlag, PeerId};
use hooks_game::{GameInfo, LeaveReason, INVALID_PLAYER_ID};

type MyHost = async::Host<lag_loss::Host<enet::Host>, net::time::Time>;

#[derive(Debug)]
pub enum Error {
    InvalidChannel,
    ConnectedTwice,
    NotConnected,
    InvalidReady,
    EnetTransport(enet::Error),
    AsyncTransport(async::Error),
    BitManager(bit_manager::Error),
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
}

impl Client {
    pub fn new(name: String) -> Client {
        Client {
            name,
            state: ClientState::Connected,
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

    /// Clients are registered here as soon as we accept their `WishConnect` message.
    clients: BTreeMap<PeerId, Client>,

    queued_events: VecDeque<Event>,
}

#[derive(Debug, Clone)]
pub enum Event {
    PlayerJoined(PeerId, String),
    PlayerLeft(PeerId, LeaveReason),
    ClientGameMsg(PeerId, ClientGameMsg, Instant),
}

// TODO: Transport peer count?
pub const PEER_COUNT: usize = 64;

impl Host {
    pub fn create(port: u16, game_info: &GameInfo) -> Result<Host, Error> {
        let host = enet::Host::create_server(port, PEER_COUNT, NUM_CHANNELS, 0, 0)?;
        let host = lag_loss::Host::new(
            host,
            lag_loss::Config {
                lag: Duration::from_millis(50),
                loss: 0.0,
            },
        );
        let host = async::Host::spawn(host);

        Ok(Host {
            host,
            game_info: game_info.clone(),
            clients: BTreeMap::new(),
            queued_events: VecDeque::new(),
        })
    }

    fn on_disconnect(&mut self, peer_id: PeerId, reason: LeaveReason) -> Option<Event> {
        if let btree_map::Entry::Occupied(entry) = self.clients.entry(peer_id) {
            debug!("Disconnecting peer {}", peer_id);

            // We have accepted this client. Does the game logic know of it?
            let ingame = entry.get().ingame();

            entry.remove();

            if ingame {
                // The player is already known to the game logic
                Some(Event::PlayerLeft(peer_id, reason))
            } else {
                None
            }
        } else {
            // Player is not fully connected
            info!("No event for disconnect from player {}", peer_id);

            None
        }
    }

    pub fn force_disconnect(&mut self, peer_id: PeerId, reason: LeaveReason) -> Result<(), Error> {
        self.host
            .disconnect(peer_id, protocol::leave_reason_to_u32(reason))?;

        if let Some(event) = self.on_disconnect(peer_id, reason) {
            // Let game logic handle this `PlayerLeft` event with the next `service` calls
            self.queued_events.push_back(event.clone());
        }

        Ok(())
    }

    pub fn update(&mut self, delta: Duration) -> Result<(), Error> {
        let peers = self.host.peers();
        let mut locked_peers = peers.lock().unwrap();
        for (&peer_id, net_time) in locked_peers.iter_mut() {
            if !self.clients.contains_key(&peer_id) {
                // Ignore clients that we have not acknowledged yet or that have already been
                // removed from the game in calculating ping etc.
                continue;
            }

            net_time.update(&mut self.host, peer_id, delta)?;
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
                    assert!(!self.clients.contains_key(&peer_id));

                    Ok(None)
                }
                transport::Event::Receive(peer_id, channel, packet) => {
                    match self.handle_receive(peer_id, channel, packet) {
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

    pub fn get_ping_secs(&self, peer_id: PeerId) -> Option<f32> {
        let peers = self.host.peers();
        let locked_peers = peers.lock().unwrap();

        locked_peers.get(&peer_id).map(|time| time.ping_secs())
    }

    pub fn send_comm(&mut self, receiver_id: PeerId, msg: &ServerCommMsg) -> Result<(), Error> {
        let data = {
            let mut writer = BitWriter::new(Vec::new());
            writer.write(msg)?;
            writer.into_inner()?
        };

        Ok(self.host
            .send(receiver_id, CHANNEL_COMM, PacketFlag::Reliable, data)?)
    }

    pub fn send_game(&mut self, receiver_id: PeerId, data: Vec<u8>) -> Result<(), Error> {
        assert!(self.clients[&receiver_id].ingame());

        Ok(self.host
            .send(receiver_id, CHANNEL_GAME, PacketFlag::Unsequenced, data)?)
    }

    fn handle_receive(
        &mut self,
        peer_id: PeerId,
        channel: ChannelId,
        packet: <MyHost as transport::Host>::Packet,
    ) -> Result<Option<Event>, Error> {
        assert!(peer_id != INVALID_PLAYER_ID);

        if channel == CHANNEL_COMM {
            // Communication messages are handled here
            let msg = {
                let mut reader = BitReader::new(packet.data());
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
                            game_info: self.game_info.clone(),
                        };
                        self.send_comm(peer_id, &reply)?;

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
        } else if channel == CHANNEL_GAME {
            // Game messages are relayed as events
            if let Some(client) = self.clients.get(&peer_id) {
                if client.ingame() {
                    let msg = {
                        let mut reader = BitReader::new(packet.data());
                        reader.read::<ClientGameMsg>()?
                    };

                    Ok(Some(Event::ClientGameMsg(
                        peer_id,
                        msg,
                        packet.receive_instant(),
                    )))
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

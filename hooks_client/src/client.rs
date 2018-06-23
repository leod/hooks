use std::f32;
use std::time::{Duration, Instant};

use bit_manager::{self, BitRead, BitReader, BitWrite, BitWriter};

use hooks_util::stats;

use hooks_game::net::protocol::{ClientCommMsg, ClientGameMsg, ServerCommMsg, CHANNEL_COMM,
                                CHANNEL_GAME, NUM_CHANNELS};
use hooks_game::net::transport::{self, async, enet, lag_loss};
use hooks_game::net::transport::{Host, Packet, PacketFlag, PeerId};
use hooks_game::net::{self, protocol};
use hooks_game::{GameInfo, LeaveReason, PlayerId};

type MyHost = async::Host<lag_loss::Host<enet::Host>, net::time::Time>;

#[derive(Debug)]
pub enum Error {
    FailedToConnect(String),
    InvalidChannel(u8),
    UnexpectedConnect,
    UnexpectedCommMsg,
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

pub struct Client {
    host: MyHost,
    peer_id: PeerId,
    game_info: GameInfo,
    ready: bool,
}

pub enum Event {
    Disconnected,
    ServerGameMsg(Vec<u8>, Instant),
}

impl Client {
    pub fn connect(host: &str, port: u16, name: &str, timeout_ms: u32) -> Result<Client, Error> {
        let address = enet::Address::create(host, port)?;

        let mut host = enet::Host::create_client(NUM_CHANNELS, 0, 0)?;
        host.connect(&address, NUM_CHANNELS)?;

        let host = lag_loss::Host::new(
            host,
            lag_loss::Config {
                lag: Duration::from_millis(0),
                loss: 0.0,
            },
        );

        let mut host = async::Host::spawn(host);

        if let Some(transport::Event::Connect(peer_id)) = host.service(timeout_ms)? {
            // Send connection request
            let msg = ClientCommMsg::WishConnect {
                name: name.to_string(),
            };
            Self::send_comm(&mut host, peer_id, &msg)?;

            // Wait for accept message
            let start_instant = Instant::now();
            while start_instant.elapsed() < Duration::from_millis(timeout_ms.into()) {
                match host.service(10)? {
                    Some(transport::Event::Receive(_, channel, packet)) => {
                        if channel != CHANNEL_COMM {
                            // Ignore any unreliable messages while connecting
                            continue;
                        }

                        let reply = Self::read_comm(packet.data())?;
                        return match reply {
                            ServerCommMsg::AcceptConnect { game_info } => {
                                // We are in!
                                Ok(Client {
                                    host,
                                    peer_id,
                                    game_info,
                                    ready: false,
                                })
                            }
                            reply => Err(Error::FailedToConnect(format!(
                                "received message {:?} instead of accepted connection",
                                reply
                            ))),
                        };
                    }
                    Some(_) => {
                        return Err(Error::FailedToConnect(
                            "did not receive message after connection wish".to_string(),
                        ))
                    }
                    None => {}
                }
            }
            Err(Error::FailedToConnect(
                "timeout while waiting for reply after connection wish".to_string(),
            ))
        } else {
            Err(Error::FailedToConnect(
                "could not connect to server".to_string(),
            ))
        }
    }

    pub fn game_info(&self) -> &GameInfo {
        &self.game_info
    }

    /// This should be called after all resources of the game have been loaded. We tell the server
    /// that we are ready to join the game. As a result, we hope to get a `JoinGame` message,
    /// including our player id.
    pub fn ready(&mut self, timeout_ms: u32) -> Result<PlayerId, Error> {
        assert!(!self.ready, "already ready");

        self.ready = true;

        Self::send_comm(&mut self.host, self.peer_id, &ClientCommMsg::Ready)?;

        // Wait for accept message
        let start_instant = Instant::now();
        while start_instant.elapsed() < Duration::from_millis(timeout_ms.into()) {
            match self.host.service(10)? {
                Some(transport::Event::Receive(_, channel, packet)) => {
                    if channel != CHANNEL_COMM {
                        // Ignore any unreliable messages while connecting
                        continue;
                    }

                    let reply = Self::read_comm(packet.data())?;
                    return match reply {
                        ServerCommMsg::JoinGame { your_player_id } => Ok(your_player_id),
                        reply => Err(Error::FailedToConnect(format!(
                            "received message {:?} instead of accepted join",
                            reply
                        ))),
                    };
                }
                Some(_) => {
                    return Err(Error::FailedToConnect(
                        "did not receive message after wanting to join game".to_string(),
                    ))
                }
                None => {}
            }
        }
        Err(Error::FailedToConnect(
            "timeout while waiting for reply after wanting to join game".to_string(),
        ))
    }

    pub fn update(&mut self, delta: Duration) -> Result<(), Error> {
        let peers = self.host.peers();
        let mut locked_peers = peers.lock().unwrap();
        for (&peer_id, net_time) in locked_peers.iter_mut() {
            net_time.update(&mut self.host, peer_id, delta)?;

            stats::record("ping", net_time.last_ping_secs().unwrap_or(f32::NAN));
        }
        Ok(())
    }

    pub fn service(&mut self) -> Result<Option<Event>, Error> {
        assert!(self.ready, "must be ready to service");

        if let Some(event) = self.host.service(0)? {
            match event {
                transport::Event::Connect(_peer_id) => Err(Error::UnexpectedConnect),
                transport::Event::Receive(_peer_id, channel, packet) => {
                    if channel == CHANNEL_COMM {
                        // Communication messages are handled here
                        let msg = Self::read_comm(packet.data())?;

                        match msg {
                            ServerCommMsg::AcceptConnect { .. } => Err(Error::UnexpectedCommMsg),
                            ServerCommMsg::JoinGame { .. } => Err(Error::UnexpectedCommMsg),
                        }
                    } else if channel == CHANNEL_GAME {
                        // Game messages are relayed
                        // TODO: Vec copy due to dropping packet
                        Ok(Some(Event::ServerGameMsg(
                            packet.data().to_vec(),
                            packet.receive_instant(),
                        )))
                    } else {
                        Err(Error::InvalidChannel(channel))
                    }
                }
                transport::Event::Disconnect(_peer) => Ok(Some(Event::Disconnected)),
            }
        } else {
            // No transport event
            Ok(None)
        }
    }

    pub fn ping_secs(&self) -> f32 {
        let peers = self.host.peers();
        let locked_peers = peers.lock().unwrap();

        locked_peers.get(&self.peer_id).unwrap().ping_secs()
    }

    fn send_comm(host: &mut MyHost, peer_id: PeerId, msg: &ClientCommMsg) -> Result<(), Error> {
        let data = {
            let mut writer = BitWriter::new(Vec::new());
            writer.write(msg)?;
            writer.into_inner()?
        };

        Ok(host.send(peer_id, CHANNEL_COMM, PacketFlag::Unsequenced, data)?)
    }

    pub fn send_game(&mut self, msg: &ClientGameMsg) -> Result<(), Error> {
        let data = {
            let mut writer = BitWriter::new(Vec::new());
            writer.write(msg)?;
            writer.into_inner()?
        };

        Ok(self.host
            .send(self.peer_id, CHANNEL_GAME, PacketFlag::Unsequenced, data)?)
    }

    fn read_comm(data: &[u8]) -> Result<ServerCommMsg, bit_manager::Error> {
        let mut reader = BitReader::new(data);
        reader.read::<ServerCommMsg>()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // TODO: Should perhaps instead disconnect reliably in a separate function
        if self.host.is_peer(self.peer_id) {
            if let Err(error) = self.host.disconnect(
                self.peer_id,
                protocol::leave_reason_to_u32(LeaveReason::Disconnected),
            ) {
                warn!("Failed to disconnect while dropping client: {:?}", error);
            }
        }
        if let Err(error) = self.host.flush() {
            warn!("Failed to flush host while dropping client: {:?}", error);
        }
    }
}

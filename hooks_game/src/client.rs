use std::time::Duration;

use bit_manager::{self, BitRead, BitReader, BitWrite, BitWriter};

use hooks_common::net::protocol::{ClientCommMsg, ClientGameMsg, ServerCommMsg, CHANNEL_COMM,
                                  CHANNEL_GAME, CHANNEL_TIME, NUM_CHANNELS};
use hooks_common::net::transport::{self, enet, Host, Packet, PacketFlag, Peer, PeerId, Transport};
use hooks_common::net::{self, protocol, DefaultTransport};
use hooks_common::{GameInfo, LeaveReason, PlayerId};

type MyTransport = DefaultTransport;

#[derive(Debug)]
pub enum Error {
    FailedToConnect(String),
    InvalidChannel(u8),
    UnexpectedConnect,
    UnexpectedCommMsg,
    Time(net::time::Error<MyTransport>),
    Transport(<MyTransport as Transport>::Error),
    BitManager(bit_manager::Error),
}

impl From<net::time::Error<MyTransport>> for Error {
    fn from(error: net::time::Error<MyTransport>) -> Error {
        Error::Time(error)
    }
}

impl From<enet::Error> for Error {
    fn from(error: <MyTransport as Transport>::Error) -> Error {
        Error::Transport(error)
    }
}

impl From<bit_manager::Error> for Error {
    fn from(error: bit_manager::Error) -> Error {
        Error::BitManager(error)
    }
}

pub struct Client {
    host: <MyTransport as Transport>::Host,
    peer_id: PeerId,
    my_player_id: PlayerId,
    game_info: GameInfo,
    ready: bool,
    net_time: net::time::Time,
}

pub enum Event {
    Disconnected,
    ServerGameMsg(Vec<u8>),
}

impl Client {
    pub fn connect(host: &str, port: u16, name: &str, timeout_ms: u32) -> Result<Client, Error> {
        let address = enet::Address::create(host, port)?;
        let mut host = enet::Host::create_client(NUM_CHANNELS, 0, 0)?;
        host.connect(&address, NUM_CHANNELS)?;

        if let Some(transport::Event::Connect(peer_id)) = host.service(timeout_ms)? {
            // Send connection request
            let msg = ClientCommMsg::WishConnect {
                name: name.to_string(),
            };
            Self::send_comm(&mut host, peer_id, msg)?;

            // Wait for accept message
            if let Some(transport::Event::Receive(_, channel, packet)) = host.service(timeout_ms)? {
                if channel != CHANNEL_COMM {
                    // FIXME: In a bit of a funny situation, this is not actually an error, because
                    // comm messages are sent reliably, while game messages are sent unreliably.
                    // Thus, it can happen that we receive a game message before receiving the
                    // acknowledgement of our connection request. So, what should we do here? Queue
                    // the game messages and loop until we get a comm message?
                    //
                    // UPDATE: no, of course we can just ignore the game messages since they are
                    // sent unreliably (TODO).
                    return Err(Error::InvalidChannel(channel));
                }

                let reply = Self::read_comm(packet.data())?;

                #[allow(unreachable_patterns)]
                match reply {
                    ServerCommMsg::AcceptConnect {
                        your_id: my_player_id,
                        game_info,
                    } => {
                        // We are in!
                        Ok(Client {
                            host,
                            peer_id,
                            my_player_id,
                            game_info,
                            ready: false,
                            net_time: net::time::Time::new(),
                        })
                    }
                    reply => Err(Error::FailedToConnect(format!(
                        "received message {:?} instead of accepted connection",
                        reply
                    ))),
                }
            } else {
                Err(Error::FailedToConnect(
                    "did not receive message after connection wish".to_string(),
                ))
            }
        } else {
            Err(Error::FailedToConnect(
                "could not connect to server".to_string(),
            ))
        }
    }

    pub fn my_player_id(&self) -> PlayerId {
        self.my_player_id
    }

    pub fn game_info(&self) -> &GameInfo {
        &self.game_info
    }

    pub fn ready(&mut self) -> Result<(), Error> {
        assert!(!self.ready, "already ready");

        self.ready = true;
        Self::send_comm(&mut self.host, self.peer_id, ClientCommMsg::Ready)
    }

    pub fn update(&mut self, delta: Duration) -> Result<(), Error> {
        return Ok(());
        self.net_time
            .update(self.host.get_peer(self.peer_id).unwrap(), delta)?;
        Ok(())
    }

    pub fn service(&mut self) -> Result<Option<Event>, Error> {
        assert!(self.ready, "must be ready to service");

        if let Some(event) = self.host.service(0)? {
            match event {
                transport::Event::Connect(_peer_id) => Err(Error::UnexpectedConnect),
                transport::Event::Receive(peer_id, channel, packet) => {
                    if channel == CHANNEL_COMM {
                        // Communication messages are handled here
                        let msg = Self::read_comm(packet.data())?;

                        match msg {
                            ServerCommMsg::AcceptConnect { .. } => Err(Error::UnexpectedCommMsg),
                        }
                    } else if channel == CHANNEL_TIME {
                        self.net_time
                            .receive(self.host.get_peer(peer_id).unwrap(), packet.data())?;
                        Ok(None)
                    } else if channel == CHANNEL_GAME {
                        // Game messages are relayed
                        Ok(Some(Event::ServerGameMsg(packet.data().to_vec())))
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

    pub fn flush(&mut self) -> Result<(), Error> {
        self.host.flush();
        Ok(())
    }

    fn send_comm(
        host: &mut <MyTransport as Transport>::Host,
        peer_id: PeerId,
        msg: ClientCommMsg,
    ) -> Result<(), Error> {
        let data = {
            let mut writer = BitWriter::new(Vec::new());
            writer.write(&msg)?;
            writer.into_inner()?
        };

        Ok(host.send(peer_id, CHANNEL_COMM, PacketFlag::Unsequenced, &data)?)
    }

    pub fn send_game(&mut self, msg: ClientGameMsg) -> Result<(), Error> {
        let data = {
            let mut writer = BitWriter::new(Vec::new());
            writer.write(&msg)?;
            writer.into_inner()?
        };

        Ok(self.host
            .send(self.peer_id, CHANNEL_GAME, PacketFlag::Unsequenced, &data)?)
    }

    fn read_comm(data: &[u8]) -> Result<ServerCommMsg, bit_manager::Error> {
        let mut reader = BitReader::new(data);
        reader.read::<ServerCommMsg>()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // TODO: Should perhaps instead disconnect reliably in a separate function
        if let Some(peer) = self.host.get_peer(self.peer_id) {
            peer.disconnect(protocol::leave_reason_to_u32(LeaveReason::Disconnected));
        }
        self.host.flush();
    }
}

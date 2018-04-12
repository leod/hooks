use std::time::Duration;

use bit_manager::{self, BitRead, BitReader, BitWrite, BitWriter};

use hooks_common::net::protocol::{self, ClientCommMsg, ClientGameMsg, ServerCommMsg, CHANNEL_COMM,
                                  CHANNEL_GAME, CHANNEL_TIME, NUM_CHANNELS};
use hooks_common::net::{self, transport};
use hooks_common::{GameInfo, LeaveReason, PlayerId};

#[derive(Debug)]
pub enum Error {
    FailedToConnect(String),
    InvalidChannel(u8),
    UnexpectedConnect,
    UnexpectedCommMsg,
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

pub struct Client {
    host: transport::Host,
    peer: transport::Peer,
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
        let address = transport::Address::create(host, port)?;
        let host = transport::Host::create_client(NUM_CHANNELS, 0, 0)?;
        let peer = host.connect(&address, NUM_CHANNELS)?;

        if let Some(transport::Event::Connect(_)) = host.service(timeout_ms)? {
            // Send connection request
            let msg = ClientCommMsg::WishConnect {
                name: name.to_string(),
            };
            Self::send_comm(&peer, msg)?;

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

                let reply = Self::read_comm(packet)?;

                #[allow(unreachable_patterns)]
                match reply {
                    ServerCommMsg::AcceptConnect {
                        your_id: my_player_id,
                        game_info,
                    } => {
                        // We are in!
                        Ok(Client {
                            host,
                            peer,
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
        Self::send_comm(&self.peer, ClientCommMsg::Ready)
    }

    pub fn update(&mut self, delta: Duration) -> Result<(), Error> {
        self.net_time.update(&self.peer, delta)?;
        Ok(())
    }

    pub fn service(&mut self) -> Result<Option<Event>, Error> {
        assert!(self.ready, "must be ready to service");

        if let Some(event) = self.host.service(0)? {
            match event {
                transport::Event::Connect(_peer) => Err(Error::UnexpectedConnect),
                transport::Event::Receive(peer, channel, packet) => {
                    if channel == CHANNEL_COMM {
                        // Communication messages are handled here
                        let msg = Self::read_comm(packet)?;

                        match msg {
                            ServerCommMsg::AcceptConnect { .. } => Err(Error::UnexpectedCommMsg),
                        }
                    } else if channel == CHANNEL_TIME {
                        self.net_time.receive(&peer, packet)?;
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

    pub fn flush(&self) -> Result<(), Error> {
        self.host.flush();
        Ok(())
    }

    fn send_comm(peer: &transport::Peer, msg: ClientCommMsg) -> Result<(), Error> {
        let data = {
            let mut writer = BitWriter::new(Vec::new());
            writer.write(&msg)?;
            writer.into_inner()?
        };

        let packet = transport::Packet::create(&data, transport::PacketFlag::Unsequenced)?;

        Ok(peer.send(CHANNEL_COMM, packet)?)
    }

    pub fn send_game(&self, msg: ClientGameMsg) -> Result<(), Error> {
        let data = {
            let mut writer = BitWriter::new(Vec::new());
            writer.write(&msg)?;
            writer.into_inner()?
        };

        let packet = transport::Packet::create(&data, transport::PacketFlag::Unsequenced)?;

        Ok(self.peer.send(CHANNEL_GAME, packet)?)
    }

    fn read_comm(packet: transport::ReceivedPacket) -> Result<ServerCommMsg, bit_manager::Error> {
        let mut reader = BitReader::new(packet.data());
        reader.read::<ServerCommMsg>()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // TODO: Should perhaps instead disconnect reliably in a separate function
        self.peer
            .disconnect(protocol::leave_reason_to_u32(LeaveReason::Disconnected));
        self.host.flush();
    }
}

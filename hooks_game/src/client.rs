use bit_manager::{self, BitRead, BitReader, BitWrite, BitWriter};

use common::{GameInfo, PlayerId};
use common::net::protocol::{ClientCommMsg, ClientGameMsg, ServerCommMsg, CHANNEL_COMM,
                            CHANNEL_GAME, NUM_CHANNELS};
use common::net::transport;

#[derive(Debug)]
pub enum Error {
    FailedToConnect(String),
    InvalidChannel(u8),
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

pub struct Client {
    host: transport::Host,
    peer: transport::Peer,
    my_id: PlayerId,
    game_info: GameInfo,
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
                    return Err(Error::InvalidChannel(channel));
                }

                let reply = Self::read_comm(packet)?;

                match reply {
                    ServerCommMsg::AcceptConnect {
                        your_id: my_id,
                        game_info,
                    } => {
                        // We are in!
                        Ok(Client {
                            host,
                            peer,
                            my_id,
                            game_info,
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

    fn send_comm(peer: &transport::Peer, msg: ClientCommMsg) -> Result<(), Error> {
        let data = {
            let mut writer = BitWriter::new(Vec::new());
            writer.write(&msg)?;
            writer.into_inner()?
        };

        let packet = transport::Packet::create(&data, transport::PacketFlag::Reliable)?;

        Ok(peer.send(CHANNEL_COMM, packet)?)
    }

    fn read_comm(packet: transport::ReceivedPacket) -> Result<ServerCommMsg, bit_manager::Error> {
        let mut reader = BitReader::new(packet.data());
        reader.read::<ServerCommMsg>()
    }
}

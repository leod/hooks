use std::collections::VecDeque;
use std::time::Duration;

use bit_manager::{self, BitRead, BitReader, BitWrite, BitWriter};

use hooks_util::timer::{duration_to_secs, Timer};

use net::protocol::{TimeMsg, CHANNEL_TIME};
use net::transport::{self, Peer, ReceivedPacket};

pub const SEND_PING_HZ: f32 = 0.5;
pub const NUM_PING_SAMPLES: usize = 20;

#[derive(Debug)]
pub enum Error {
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

pub struct Time {
    send_ping_timer: Timer,
    local_time: f32,
    ping_samples: VecDeque<f32>,
}

impl Time {
    pub fn new() -> Time {
        Time {
            send_ping_timer: Timer::from_hz(SEND_PING_HZ),
            local_time: 0.0,
            ping_samples: VecDeque::new(),
        }
    }

    pub fn receive(&mut self, peer: &Peer, packet: ReceivedPacket) -> Result<(), Error> {
        let mut reader = BitReader::new(packet.data());
        let msg = reader.read::<TimeMsg>()?;

        match msg {
            TimeMsg::Ping { send_time } => {
                Self::send(
                    peer,
                    TimeMsg::Pong {
                        ping_send_time: send_time,
                    },
                )?;
            }
            TimeMsg::Pong { ping_send_time } => {
                if ping_send_time <= self.local_time {
                    // TODO: Might want to do some more sanity checking here, since otherwise peers
                    //       can fake their pings. For example, use sequence numbers instead of
                    //       sending the send times.
                    let ping = self.local_time - ping_send_time;
                    debug!("ping: {:.2}ms", ping * 1000.0);
                    self.ping_samples.push_back(ping);

                    if self.ping_samples.len() > NUM_PING_SAMPLES {
                        self.ping_samples.pop_front();
                    }
                }
            }
        }

        Ok(())
    }

    pub fn update(&mut self, peer: &Peer, delta: Duration) -> Result<(), Error> {
        self.local_time += duration_to_secs(delta);
        self.send_ping_timer += delta;

        if self.send_ping_timer.trigger_reset() {
            Self::send(
                peer,
                TimeMsg::Ping {
                    send_time: self.local_time,
                },
            )?;
        }

        Ok(())
    }

    fn send(peer: &Peer, msg: TimeMsg) -> Result<(), Error> {
        let data = {
            let mut writer = BitWriter::new(Vec::new());
            writer.write(&msg)?;
            writer.into_inner()?
        };

        // We send as unsequenced, unreliable packets so that we get the same delivery times as for
        // the CHANNEL_GAME messages
        let packet = transport::Packet::create(&data, transport::PacketFlag::Unsequenced)?;

        peer.send(CHANNEL_TIME, packet)
            .map_err(|error| Error::Transport(error))
    }
}

use std::collections::VecDeque;
use std::fmt::Debug;
use std::time::Duration;

use bit_manager::{self, BitRead, BitReader, BitWrite, BitWriter};

use hooks_util::timer::{duration_to_secs, Timer};

use net::protocol::{TimeMsg, CHANNEL_TIME};
use net::transport::{Host, PacketFlag, PeerId};

pub const SEND_PING_HZ: f32 = 0.5;
pub const NUM_PING_SAMPLES: usize = 20;

#[derive(Debug)]
pub enum Error<E: Debug> {
    Transport(E),
    BitManager(bit_manager::Error),
}

impl<E: Debug> From<bit_manager::Error> for Error<E> {
    fn from(error: bit_manager::Error) -> Error<E> {
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

    pub fn receive<H: Host>(
        &mut self,
        host: &mut H,
        peer_id: PeerId,
        data: &[u8],
    ) -> Result<(), Error<H::Error>> {
        let mut reader = BitReader::new(data);
        let msg = reader.read::<TimeMsg>()?;

        match msg {
            TimeMsg::Ping { send_time } => {
                Time::send(
                    host,
                    peer_id,
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

    pub fn update<H: Host>(
        &mut self,
        host: &mut H,
        peer_id: PeerId,
        delta: Duration,
    ) -> Result<(), Error<H::Error>> {
        self.local_time += duration_to_secs(delta);
        self.send_ping_timer += delta;

        if self.send_ping_timer.trigger_reset() {
            Time::send(
                host,
                peer_id,
                TimeMsg::Ping {
                    send_time: self.local_time,
                },
            )?;
        }

        Ok(())
    }

    fn send<H: Host>(host: &mut H, peer_id: PeerId, msg: TimeMsg) -> Result<(), Error<H::Error>> {
        let data = {
            let mut writer = BitWriter::new(Vec::new());
            writer.write(&msg)?;
            writer.into_inner()?
        };

        // We send as unsequenced, unreliable packets so that we get the same delivery times as for
        // the CHANNEL_GAME messages
        host.send(peer_id, CHANNEL_TIME, PacketFlag::Unsequenced, &data)
            .map_err(|error| Error::Transport(error))
    }
}

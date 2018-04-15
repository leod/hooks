use std::collections::VecDeque;
use std::time::Duration;

use bit_manager::{self, BitRead, BitReader, BitWrite, BitWriter};

use hooks_util::timer::{duration_to_secs, Timer};

use net::protocol::{TimeMsg, CHANNEL_TIME};
use net::transport::{PacketFlag, Peer, Transport};

pub const SEND_PING_HZ: f32 = 0.5;
pub const NUM_PING_SAMPLES: usize = 20;

#[derive(Debug)]
pub enum Error<T: Transport> {
    Transport(T::Error),
    BitManager(bit_manager::Error),
}

impl<T: Transport> From<bit_manager::Error> for Error<T> {
    fn from(error: bit_manager::Error) -> Error<T> {
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

    pub fn receive<P: Peer>(
        &mut self,
        peer: &mut P,
        data: &[u8],
    ) -> Result<(), Error<P::Transport>> {
        let mut reader = BitReader::new(data);
        let msg = reader.read::<TimeMsg>()?;

        match msg {
            TimeMsg::Ping { send_time } => {
                Time::send(
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

    pub fn update<P: Peer>(
        &mut self,
        peer: &mut P,
        delta: Duration,
    ) -> Result<(), Error<P::Transport>> {
        self.local_time += duration_to_secs(delta);
        self.send_ping_timer += delta;

        if self.send_ping_timer.trigger_reset() {
            Time::send(
                peer,
                TimeMsg::Ping {
                    send_time: self.local_time,
                },
            )?;
        }

        Ok(())
    }

    fn send<P: Peer>(peer: &mut P, msg: TimeMsg) -> Result<(), Error<P::Transport>> {
        let data = {
            let mut writer = BitWriter::new(Vec::new());
            writer.write(&msg)?;
            writer.into_inner()?
        };

        // We send as unsequenced, unreliable packets so that we get the same delivery times as for
        // the CHANNEL_GAME messages
        peer.send(CHANNEL_TIME, PacketFlag::Unsequenced, &data)
            .map_err(|error| Error::Transport(error))
    }
}

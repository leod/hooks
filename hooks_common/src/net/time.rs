use std::collections::VecDeque;
use std::time::{Duration, Instant};

use bit_manager::{BitRead, BitReader, BitWrite, BitWriter};

use hooks_util::timer::{duration_to_secs, Timer};

use net::protocol::{TimeMsg, CHANNEL_TIME};
use net::transport::async::PeerData;
use net::transport::{ChannelId, Host, PacketFlag, PeerId};

pub const SEND_PING_HZ: f32 = 0.5;
pub const NUM_PING_SAMPLES: usize = 20;

#[derive(Debug)]
pub struct Time {
    start_instant: Instant,
    send_ping_timer: Timer,
    ping_samples: VecDeque<f32>,
}

impl Default for Time {
    fn default() -> Time {
        Time {
            start_instant: Instant::now(),
            send_ping_timer: Timer::from_hz(SEND_PING_HZ),
            ping_samples: VecDeque::new(),
        }
    }
}

impl PeerData for Time {
    fn receive<H: Host>(
        &mut self,
        host: &mut H,
        peer_id: PeerId,
        channel_id: ChannelId,
        data: &[u8],
    ) -> Result<bool, H::Error> {
        if channel_id == CHANNEL_TIME {
            let mut reader = BitReader::new(data);
            let msg = reader.read::<TimeMsg>();
            let msg = match msg {
                Ok(msg) => msg,
                Err(error) => {
                    // TODO: Properly propagate async transport errors
                    debug!(
                        "Received malformed time message from peer {} ({} bytes), ignoring: {:?}",
                        data.len(),
                        peer_id,
                        error
                    );
                    return Ok(true);
                }
            };

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
                    let now = Instant::now();
                    let pong_receive_time =
                        duration_to_secs(now.duration_since(self.start_instant));

                    if ping_send_time <= pong_receive_time {
                        // TODO: Might want to do some more sanity checking here, since otherwise peers
                        //       can fake their pings. For example, use sequence numbers instead of
                        //       sending the send times.
                        let ping = pong_receive_time - ping_send_time;
                        //println!("ping: {:.2}ms", ping * 1000.0);
                        self.ping_samples.push_back(ping);

                        if self.ping_samples.len() > NUM_PING_SAMPLES {
                            self.ping_samples.pop_front();
                        }
                    }
                }
            }

            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl Time {
    pub fn update<H: Host>(
        &mut self,
        host: &mut H,
        peer_id: PeerId,
        delta: Duration,
    ) -> Result<(), H::Error> {
        self.send_ping_timer += delta;

        let now = Instant::now();
        let send_time = duration_to_secs(now.duration_since(self.start_instant));

        while self.send_ping_timer.trigger() {
            Time::send(host, peer_id, TimeMsg::Ping { send_time })?;
        }

        Ok(())
    }

    pub fn last_ping(&self) -> Option<f32> {
        self.ping_samples.back().map(|t| *t)
    }

    fn send<H: Host>(host: &mut H, peer_id: PeerId, msg: TimeMsg) -> Result<(), H::Error> {
        let data = {
            let mut writer = BitWriter::new(Vec::new());
            writer.write(&msg).unwrap();
            writer.into_inner().unwrap()
        };

        // We send as unsequenced, unreliable packets so that we get the same delivery times as for
        // the CHANNEL_GAME messages
        host.send(peer_id, CHANNEL_TIME, PacketFlag::Unsequenced, data)
    }
}

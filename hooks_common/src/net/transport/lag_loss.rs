//! Transport wrapper for simulating lag and loss.
//! TODO: Variance in lag and loss.

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::mem;
use std::time::{Duration, Instant};

use net::transport::{self, ChannelId, Event, PacketFlag, PeerId};

#[derive(PartialEq, Eq)]
enum Command {
    Send(PeerId, ChannelId, PacketFlag, Vec<u8>),
    Disconnect(PeerId, u32),
}

#[derive(PartialEq, Eq)]
struct Payload {
    time: Instant,
    command: Command,
}

pub struct Config {
    pub lag: Duration,
    pub loss: f32,
}

pub struct Host<H: transport::Host> {
    host: H,
    config: Config,
    queue: BinaryHeap<Payload>,
}

impl Ord for Payload {
    fn cmp(&self, other: &Payload) -> Ordering {
        other.time.cmp(&self.time)
    }
}

impl PartialOrd for Payload {
    fn partial_cmp(&self, other: &Payload) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<H: transport::Host> transport::Host for Host<H> {
    type Error = H::Error;
    type Packet = H::Packet;

    fn is_peer(&self, id: PeerId) -> bool {
        self.host.is_peer(id)
    }

    fn service(&mut self, timeout_ms: u32) -> Result<Option<Event<Self::Packet>>, Self::Error> {
        self.flush()?;

        // For now, lag/loss is applied only to outgoing data
        self.host.service(timeout_ms)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        let now = Instant::now();

        while let Some(time) = self.queue.peek().map(|p| p.time) {
            if now < time {
                break;
            }

            let command = self.queue.pop().unwrap().command;
            self.run_command(command)?;
        }

        self.host.flush()?;

        Ok(())
    }

    fn send(
        &mut self,
        peer_id: PeerId,
        channel_id: ChannelId,
        flag: PacketFlag,
        data: Vec<u8>,
    ) -> Result<(), Self::Error> {
        let now = Instant::now();
        self.queue.push(Payload {
            time: now + self.config.lag,
            command: Command::Send(peer_id, channel_id, flag, data),
        });
        Ok(())
    }

    fn disconnect(&mut self, peer_id: PeerId, data: u32) -> Result<(), Self::Error> {
        let now = Instant::now();
        self.queue.push(Payload {
            time: now + self.config.lag,
            command: Command::Disconnect(peer_id, data),
        });
        Ok(())
    }
}

impl<H: transport::Host> Host<H> {
    pub fn new(host: H, config: Config) -> Host<H> {
        assert!(config.loss == 0.0, "loss not implemented yet");

        Host {
            host,
            config,
            queue: BinaryHeap::new(),
        }
    }

    fn run_command(&mut self, command: Command) -> Result<(), H::Error> {
        match command {
            Command::Send(peer_id, channel_id, packet_flag, data) => {
                if self.host.is_peer(peer_id) {
                    self.host.send(peer_id, channel_id, packet_flag, data)?;
                } else {
                    // The peer might already have disconnected
                    debug!("Ignoring send command to invalid peer_id {}", peer_id);
                }
            }
            Command::Disconnect(peer_id, data) => {
                if self.host.is_peer(peer_id) {
                    self.host.disconnect(peer_id, data)?;
                } else {
                    // The peer might already have disconnected
                    debug!("Ignoring disconnect command to invalid peer_id {}", peer_id);
                }
            }
        }
        Ok(())
    }
}

impl<H: transport::Host> Drop for Host<H> {
    fn drop(&mut self) {
        let queue = mem::replace(&mut self.queue, BinaryHeap::new());
        for payload in queue {
            if let Err(error) = self.run_command(payload.command) {
                warn!("Failed to run command while dropping host: {:?}", error);
            }
        }
        if let Err(error) = self.host.flush() {
            warn!("Failed to flush while dropping host: {:?}", error);
        }
    }
}

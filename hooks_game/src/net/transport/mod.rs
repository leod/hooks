pub mod async;
pub mod enet;
pub mod lag_loss;

use std::time::Instant;

use std::fmt::Debug;

pub type ChannelId = u8;
pub type PeerId = u32;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PacketFlag {
    Reliable,
    Unreliable,
    Unsequenced,
}

pub enum Event<P: Packet> {
    Connect(PeerId),
    Receive(PeerId, ChannelId, P),
    Disconnect(PeerId),
}

pub trait Host {
    type Error: Debug;
    type Packet: Packet;

    fn is_peer(&self, peer_id: PeerId) -> bool;
    fn service(&mut self, timeout_ms: u32) -> Result<Option<Event<Self::Packet>>, Self::Error>;
    fn flush(&mut self) -> Result<(), Self::Error>;
    fn send(
        &mut self,
        peer_id: PeerId,
        channel_id: ChannelId,
        flag: PacketFlag,
        data: Vec<u8>,
    ) -> Result<(), Self::Error>;
    fn disconnect(&mut self, peer_id: PeerId, data: u32) -> Result<(), Self::Error>;
}

pub trait Packet {
    fn data(&self) -> &[u8];
    fn receive_instant(&self) -> Instant;
}

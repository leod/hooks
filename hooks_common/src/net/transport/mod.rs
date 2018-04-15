pub mod enet;
//pub mod async;

pub use std::fmt::Debug;

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

    fn is_peer(&self, id: PeerId) -> bool;
    fn service(&mut self, timeout_ms: u32) -> Result<Option<Event<Self::Packet>>, Self::Error>;
    fn flush(&mut self) -> Result<(), Self::Error>;
    fn disconnect(&mut self, id: PeerId, data: u32) -> Result<(), Self::Error>;
    fn send(
        &mut self,
        id: PeerId,
        channel_id: ChannelId,
        flag: PacketFlag,
        data: &[u8],
    ) -> Result<(), Self::Error>;
}

pub trait Packet {
    fn data(&self) -> &[u8];
}

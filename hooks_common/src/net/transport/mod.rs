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
    type Peer: Peer<Error = Self::Error>;
    type Packet: Packet;

    fn get_peer<'a>(&'a mut self, id: PeerId) -> Option<&'a mut Self::Peer>;

    fn service(&mut self, timeout_ms: u32) -> Result<Option<Event<Self::Packet>>, Self::Error>;

    fn flush(&mut self);

    fn send(
        &mut self,
        id: PeerId,
        channel_id: ChannelId,
        flag: PacketFlag,
        data: &[u8],
    ) -> Result<(), Self::Error> {
        self.get_peer(id).unwrap().send(channel_id, flag, data)
    }
}

pub trait Peer {
    type Error: Debug;

    fn id(&self) -> PeerId;
    fn send(
        &mut self,
        channel_id: ChannelId,
        flag: PacketFlag,
        data: &[u8],
    ) -> Result<(), Self::Error>;
    fn disconnect(&mut self, data: u32);
}

pub trait Packet {
    fn data(&self) -> &[u8];
}

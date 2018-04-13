pub mod enet;

pub use std::fmt::Debug;

pub type ChannelId = u8;
pub type PeerId = u32;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PacketFlag {
    Reliable,
    Unreliable,
    Unsequenced,
}

pub enum Event<T: Transport> {
    Connect(PeerId),
    Receive(PeerId, ChannelId, T::Packet),
    Disconnect(PeerId),
}

pub trait Transport: Sized + Debug {
    type Error: Debug;
    type Host: Host<Transport = Self>;
    type Peer: Peer<Transport = Self>;
    type Packet: Packet;
}

pub trait Host: Sized {
    type Transport: Transport<Host = Self>;

    fn get_peer<'a>(
        &'a mut self,
        id: PeerId,
    ) -> Option<&'a mut <Self::Transport as Transport>::Peer>;

    fn service(
        &mut self,
        timeout_ms: u32,
    ) -> Result<Option<Event<Self::Transport>>, <Self::Transport as Transport>::Error>;

    fn flush(&mut self);

    fn send(
        &mut self,
        id: PeerId,
        channel_id: ChannelId,
        flag: PacketFlag,
        data: &[u8],
    ) -> Result<(), <Self::Transport as Transport>::Error> {
        self.get_peer(id).unwrap().send(channel_id, flag, data)
    }
}

pub trait Peer: Sized {
    type Transport: Transport<Peer = Self> + ?Sized;

    fn id(&self) -> PeerId;
    fn send(
        &mut self,
        channel_id: ChannelId,
        flag: PacketFlag,
        data: &[u8],
    ) -> Result<(), <Self::Transport as Transport>::Error>;
    fn disconnect(&mut self, data: u32);
}

pub trait Packet {
    fn data(&self) -> &[u8];
}

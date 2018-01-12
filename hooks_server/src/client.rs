use common::TickNum;
use common::net::transport;

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum State {
    Connected,

    /// We forcefully disconnected the client.
    Disconnected,
}

pub struct Client {
    pub peer: transport::Peer,
    pub state: State,
    pub last_ack_tick: Option<TickNum>,
}

impl Client {
    pub fn new(peer: transport::Peer) -> Client {
        Client {
            peer,
            state: State::Connected,
            last_ack_tick: None,
        }
    }
}

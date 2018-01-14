use common::TickNum;
use common::net::transport;

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum State {
    /// We have acknowledged the connection and sent game info.
    Connected,

    /// The client has received the game info and is ready to receive ticks.
    Ready,
}

pub struct Client {
    pub peer: transport::Peer,
    pub name: String,
    pub state: State,
}

impl Client {
    pub fn new(peer: transport::Peer, name: String) -> Client {
        Client {
            peer,
            name,
            state: State::Connected,
        }
    }

    pub fn ingame(&self) -> bool {
        match self.state {
            State::Connected => false,
            State::Ready => true,
        }
    }
}

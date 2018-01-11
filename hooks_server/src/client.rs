use common::{PlayerInfo, TickNum};

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum State {
    Connected
}

#[derive(Debug)]
pub struct Client {
    pub state: State,
    pub last_ack_tick: TickNum,
}

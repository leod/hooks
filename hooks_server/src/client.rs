use common::{PlayerId, PlayerInfo, TickNum};

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum State {
    Connecting,
    InGame,
}

#[derive(Debug)]
pub struct Client {
    id: PlayerId,
    info: PlayerInfo,
    state: State,
    last_ack_tick: TickNum,
}

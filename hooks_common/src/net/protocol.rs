use defs::{GameInfo, PlayerId, PlayerInput, TickNum, TimedPlayerInput};

#[derive(Debug, Clone)]
pub enum Channel {
    /// Reliable meta messages
    Comm,

    /// Unreliable messages about the game
    Game,
}
pub const NUM_CHANNELS: usize = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientCommMsg {
    WishConnect { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerCommMsg {
    AcceptConnect {
        your_id: PlayerId,
        game_info: GameInfo,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientGameMsg {
    PlayerInput(PlayerInput),
    ReceivedTick(TickNum),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerGameMsg {

}

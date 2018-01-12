use defs::{GameInfo, PlayerId, PlayerInput, TickNum, TimedPlayerInput};

pub const CHANNEL_COMM: u8 = 0;
pub const CHANNEL_GAME: u8 = 0;
pub const NUM_CHANNELS: usize = 2;

#[derive(Debug, Clone, BitStore)]
pub enum ClientCommMsg {
    /// First message that the client should send.
    WishConnect { name: String },
}

#[derive(Debug, Clone, BitStore)]
pub enum ServerCommMsg {
    /// Response to `WishConnect`: Server accepts the connection request.
    AcceptConnect {
        your_id: PlayerId,
        game_info: GameInfo,
    },
}

#[derive(Debug, Clone, BitStore)]
pub enum ClientGameMsg {
    PlayerInput(PlayerInput),
    ReceivedTick(TickNum),
}

#[derive(Debug, Clone, BitStore)]
pub enum ServerGameMsg {
    Dummy, 
}

use defs::{GameInfo, LeaveReason, PlayerId, PlayerInput, TickNum, TimedPlayerInput};

pub const CHANNEL_COMM: u8 = 0;
pub const CHANNEL_GAME: u8 = 0;
pub const NUM_CHANNELS: usize = 2;

pub fn leave_reason_to_u32(reason: LeaveReason) -> u32 {
    match reason {
        LeaveReason::Disconnected => 0,
        LeaveReason::InvalidMsg => 666,
        LeaveReason::Lagged => 420,
    }
}

pub fn u32_to_leave_reason(n: u32) -> Option<LeaveReason> {
    match n {
        0 => Some(LeaveReason::Disconnected),
        666 => Some(LeaveReason::InvalidMsg),
        420 => Some(LeaveReason::Lagged),
        _ => None,
    }
}

#[derive(Debug, Clone, BitStore)]
pub enum ClientCommMsg {
    /// First message that the client should send.
    WishConnect { name: String },

    /// Respone to `AcceptConnect`, after loading the game.
    Ready,
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

use defs::{GameInfo, LeaveReason, PlayerId, PlayerInput, TickNum};

pub const CHANNEL_COMM: u8 = 0;
pub const CHANNEL_TIME: u8 = 1;
pub const CHANNEL_GAME: u8 = 2;
pub const NUM_CHANNELS: usize = 3;

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
pub enum TimeMsg {
    Ping { send_time: f32 },
    Pong { ping_send_time: f32 },
}

#[derive(Debug, Clone, BitStore)]
pub enum ClientGameMsg {
    /// Client acknowledges having received a tick.
    ReceivedTick(TickNum),

    /// Client started a tick.
    StartedTick {
        /// Tick which the client started.
        tick: TickNum,

        /// Tick in which the client estimates that the server will execute the input. This should
        /// always be larger than `tick`. This is currently just used for debugging, but might have
        /// a future purpose in adjusting time.
        target_tick: TickNum,

        /// The input to be executed.
        input: PlayerInput,
    },
}

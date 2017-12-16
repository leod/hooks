use defs::{PlayerId, GameInfo, TimedPlayerInput, TickNumber};

#[derive(Debug, Clone)]
pub enum Channel {
    Messages,
    Ticks,
} 
pub const NUM_CHANNELS: usize = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    Pong,
    WishConnect {
        name: String,
    },
    PlayerInput(TimedPlayerInput),
    StartingTick {
        tick: TickNumber,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    Ping,
    AcceptConnect {
        your_id: PlayerId,
        game_info: GameInfo,
    },
}

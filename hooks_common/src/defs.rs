use nalgebra::Vector2;

pub type PlayerId = u32;

// Global Ids for entities shared between server and clients.
// Note that the zero Id is reserved.
pub type EntityId = u32;

pub type EntityClassId = u32;

pub type TickNum = u32;

pub const INVALID_PLAYER_ID: PlayerId = 0;
pub const INVALID_ENTITY_ID: EntityId = 0;

#[derive(Debug, Clone, BitStore)]
pub struct MapInfo {}

#[derive(Debug, Clone, Default, BitStore)]
pub struct PlayerStats {
    pub score: u32,
    pub deaths: u32,
}

#[derive(Debug, Clone, BitStore)]
pub struct PlayerInfo {
    pub name: String,
    pub stats: PlayerStats,
}

impl PlayerInfo {
    pub fn new(name: String) -> PlayerInfo {
        PlayerInfo {
            name: name,
            stats: PlayerStats::default(),
        }
    }
}

/// Sent to the clients by the server after connecting
#[derive(Debug, Clone, BitStore)]
pub struct GameInfo {
    pub ticks_per_second: u32,
    pub map_info: MapInfo,
}

#[derive(Debug, Clone, BitStore)]
pub struct PlayerInput {
    pub rot_angle: f32,
    pub shoot_one: bool,
    pub shoot_two: bool,
}

#[derive(Debug, Clone, BitStore)]
pub struct TimedPlayerInput {
    pub duration_s: f32,
    pub input: PlayerInput,
}

#[derive(Debug, Clone, BitStore)]
pub enum LeaveReason {
    InvalidMsg,        
    Disconnected,
}

#[derive(Debug, Clone, BitStore)]
pub enum DeathReason {
    Caught(PlayerId),
}

#[derive(Debug, Clone, BitStore)]
pub struct PlayerStatsUpdate;

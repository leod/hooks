use std::time::Duration;

use hooks_util::timer;

pub type PlayerId = u32;
pub type EntityIndex = u32;

/// Global Ids for entities shared between server and clients.
/// Note that the zero id is reserved.
pub type EntityId = (PlayerId, EntityIndex);

pub type EntityClassId = u32;

pub type TickNum = u32;

pub type TickDeltaNum = u8;

/// Tick is not sent as a delta snapshot
pub const NO_DELTA_TICK: TickDeltaNum = 0;

pub const INVALID_PLAYER_ID: PlayerId = 0;
pub const INVALID_ENTITY_ID: EntityId = (INVALID_PLAYER_ID, 0);

#[derive(Debug, Clone, BitStore)]
pub struct MapInfo;

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
    pub fn new(name: String) -> Self {
        Self {
            name: name,
            stats: PlayerStats::default(),
        }
    }
}

/// Sent to the clients by the server after connecting.
#[derive(Debug, Clone, BitStore)]
pub struct GameInfo {
    pub ticks_per_second: u32,
    pub ticks_per_snapshot: u32,
    pub map_info: MapInfo,
    pub player_entity_class: String,
}

impl GameInfo {
    pub fn tick_duration(&self) -> Duration {
        timer::secs_to_duration(self.tick_duration_secs())
    }

    pub fn tick_duration_secs(&self) -> f32 {
        1.0 / (self.ticks_per_second as f32)
    }
}

#[derive(Debug, Clone, Default, BitStore)]
pub struct PlayerInput {
    pub rot_angle: f32,
    pub move_forward: bool,
    pub move_backward: bool,
    pub move_left: bool,
    pub move_right: bool,
    pub shoot_one: bool,
    pub shoot_two: bool,
}

#[derive(Debug, Copy, Clone, BitStore)]
pub enum LeaveReason {
    Disconnected,
    InvalidMsg,
    Lagged,
}

#[derive(Debug, Clone, BitStore)]
pub enum DeathReason {
    Caught(PlayerId),
}

#[derive(Debug, Clone, BitStore)]
pub struct PlayerStatsUpdate;

use std::time::Duration;

use hooks_util::timer;

use net::transport::PeerId;

pub type PlayerId = PeerId;
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
            name,
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
    pub server_target_lag_inputs: TickNum,
    pub client_target_lag_snapshots: TickNum,
}

impl GameInfo {
    pub fn tick_duration(&self) -> Duration {
        timer::secs_to_duration(self.tick_duration_secs())
    }

    pub fn tick_duration_secs(&self) -> f32 {
        1.0 / (self.ticks_per_second as f32)
    }

    /// Estimate in which tick a client's input will be run on the server.
    pub fn input_target_tick(&self, ping_secs: f32, client_tick: TickNum) -> TickNum {
        // Crude estimate based on ping, I guess time synchronization stuff could help here...
        let receive_delay_ticks = (ping_secs / (2.0 * self.tick_duration_secs())).ceil() as TickNum;
        let delay_ticks = receive_delay_ticks + self.server_target_lag_inputs +
            (self.client_target_lag_snapshots + 1) * self.ticks_per_snapshot;
        client_tick + delay_ticks
    }

    /// How many ticks the client should lag behind the latest tick it received.
    pub fn client_target_lag_ticks(&self) -> TickNum {
        self.client_target_lag_snapshots * self.ticks_per_snapshot
    }
}

#[derive(Debug, Clone, PartialEq, Default, BitStore)]
pub struct PlayerInput {
    pub rot_angle: f32,
    pub move_forward: bool,
    pub move_backward: bool,
    pub move_left: bool,
    pub move_right: bool,
    pub shoot_one: bool,
    pub shoot_two: bool,
    pub pull_one: bool,
    pub pull_two: bool,
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

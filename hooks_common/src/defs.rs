use nalgebra::Vector2;

pub type PlayerId = u32;
pub type TickNumber = u32;

// Entities shared between server and clients
pub type EntityId = u32;
pub type EntityTypeId = u32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapInfo {
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlayerStats {
    pub score: u32,
    pub deaths: u32,
    pub ping_ms: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameInfo {
    pub ticks_per_second: u32,
    pub map_info: MapInfo,
    pub players: Vec<(PlayerId, PlayerInfo)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerInput {
    pub rot_angle: f32,
    pub shoot_one: bool,
    pub shoot_two: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimedPlayerInput {
    pub duration_s: f32,
    pub input: PlayerInput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeathReason {
    Caught(PlayerId),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameEvent {
    // Player list replication
    PlayerJoin(PlayerId, PlayerInfo),
    PlayerLeave(PlayerId),
    UpdatePlayerStats(Vec<(PlayerId, PlayerStats)>),

    PlayerDied {
        player_id: PlayerId,
        position: Vector2<f32>, 
        responsible_player_id: PlayerId,
        reason: DeathReason,
    },
    
    // Entity replication
    CreateEntity(EntityId, EntityTypeId, PlayerId),
    RemoveEntity(EntityId),
}

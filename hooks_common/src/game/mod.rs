pub mod entity;
pub mod state;
pub mod init;
pub mod input;
pub mod predict;
pub mod run;
pub mod catch;

use registry::Registry;
use repl;

pub use self::snapshot::{ComponentType, EntityClasses, EntitySnapshot, LoadSnapshotSys,
                         StoreSnapshotSys, WorldSnapshot};
pub use self::state::State;

fn register(_: &mut Registry) {}

pub mod auth {
    use super::*;

    pub fn register(reg: &mut Registry) {
        super::register(reg);
        repl::entity::auth::register::<EntitySnapshot>(reg);
        entity::auth::register(reg);
        catch::auth::register(reg);
    }
}

pub mod view {
    use super::*;

    pub fn register(reg: &mut Registry) {
        super::register(reg);
        repl::entity::view::register::<EntitySnapshot>(reg);
        entity::view::register(reg);
    }
}

snapshot! {
    use physics::Position;
    use physics::Orientation;
    use physics::Velocity;
    use physics::AngularVelocity;

    use game::entity::HookDef;
    use game::entity::HookSegmentDef;
    use game::entity::HookState;

    use game::entity::player::Player;

    mod snapshot {
        position: Position,
        orientation: Orientation,

        velocity: Velocity,
        angular_velocity: AngularVelocity,

        hook_def: HookDef,
        hook_segment_def: HookSegmentDef,
        hook_state: HookState,

        player: Player,
    }
}

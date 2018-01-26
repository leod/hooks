pub mod entity;
pub mod state;
pub mod init;
pub mod input;
pub mod catch;

use physics::{self, collision};
use registry::Registry;
use repl;

pub use self::snapshot::{ComponentType, EntityClasses, EntitySnapshot, LoadSnapshotSys,
                         StoreSnapshotSys, WorldSnapshot};
pub use self::state::State;

fn register(reg: &mut Registry) {
    reg.tick_system(
        collision::CreateObjectSys::new(),
        "physics::collision::CreateObjectSys",
        &[],
    );
    reg.tick_system(
        collision::UpdateSys,
        "physics::collision::UpdateSys",
        &["physics::collision::CreateObjectSys"],
    );
}

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

    mod snapshot {
        position: Position,
        orientation: Orientation,
    }
}

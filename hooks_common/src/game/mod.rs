mod test_entity;
pub mod state;

use registry::Registry;
use repl::entity;

pub use self::snapshot::{ComponentType, EntitySnapshot, WorldSnapshot};
pub use self::state::State;

fn register(_: &mut Registry) {}

pub mod auth {
    use super::*;

    pub fn register(reg: &mut Registry) {
        super::register(reg);
        entity::auth::register::<EntitySnapshot>(reg);
        test_entity::auth::register(reg);
    }
}

pub mod view {
    use super::*;

    pub fn register(reg: &mut Registry) {
        super::register(reg);
        entity::view::register::<EntitySnapshot>(reg);
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

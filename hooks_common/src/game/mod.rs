mod test_entity;

use registry::Registry;
use repl;

pub use self::snapshot::{ComponentType, EntitySnapshot, WorldSnapshot};

pub fn register(reg: &mut Registry) {
    repl::entity::register::<EntitySnapshot>(reg);
}

snapshot! {
    use physics::Position;
    use physics::Orientation;

    mod snapshot {
        position: Position,
        orientation: Orientation,
    }
}

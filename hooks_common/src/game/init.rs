use specs::World;

use repl::entity;

pub mod auth {
    use super::*;

    pub fn create_state(world: &mut World) {
        // Just some stupid entities for initial testing

        entity::auth::create(world, 0, "test", |builder| builder);
    }
}

pub mod view {
    use super::*;

    pub fn create_state(world: &mut World) {}
}

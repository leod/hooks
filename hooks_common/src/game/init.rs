use nalgebra::Point2;
use specs::World;

use physics::Position;
use repl::entity;

pub mod auth {
    use super::*;

    pub fn create_state(world: &mut World) {
        // Just some stupid entities for initial testing

        entity::auth::create(world, 0, "test", |builder| {
            builder.with(Position {
                pos: Point2::new(0.0, -50.0),
            })
        });
        entity::auth::create(world, 0, "test", |builder| {
            builder.with(Position {
                pos: Point2::origin(),
            })
        });
        entity::auth::create(world, 0, "test", |builder| {
            builder.with(Position {
                pos: Point2::new(0.0, 50.0),
            })
        });
    }
}

pub mod view {
    use super::*;

    pub fn create_state(world: &mut World) {}
}

use std::f32;

use nalgebra::{Point2, Vector2};
use rand::{self, Rng};
use specs::World;

use game::entity::wall;

fn create_state(world: &mut World) {
    let n_walls = 100;
    let mut rng = rand::thread_rng();

    for _ in 0..n_walls {
        let x = (rng.gen::<f32>() - 0.5) * 2000.0;
        let y = (rng.gen::<f32>() - 0.5) * 2000.0;
        let pos = Point2::new(x, y);

        let w = rng.gen::<f32>() * 300.0 + 20.0;
        let h = rng.gen::<f32>() * 10.0 + 1.0;
        let size = Vector2::new(w, h);

        let angle = rng.gen::<f32>() * f32::consts::PI;

        wall::create(world, pos, size, angle);
    }
}

pub mod auth {
    use physics::Position;
    use repl::entity;

    use super::*;

    pub fn create_state(world: &mut World) {
        super::create_state(world);

        // Just some stupid entities for initial testing

        entity::auth::create(world, 0, "test", |builder| {
            builder.with(Position(Point2::new(0.0, -50.0)))
        });
        entity::auth::create(world, 0, "test", |builder| {
            builder.with(Position(Point2::origin()))
        });
        entity::auth::create(world, 0, "test", |builder| {
            builder.with(Position(Point2::new(0.0, 50.0)))
        });
    }
}

pub mod view {
    use super::*;

    pub fn create_state(world: &mut World) {
        super::create_state(world);
    }
}

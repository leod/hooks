use std::f32;

use nalgebra::{Point2, Vector2};
use rand::{IsaacRng, Rng};
use specs::prelude::World;

use game::entity::wall;

fn create_state(world: &mut World) {
    let n_walls = 200;
    let mut rng = IsaacRng::new_unseeded();

    for _ in 0..n_walls {
        let x = (rng.gen::<f32>() - 0.5) * 5000.0;
        let y = (rng.gen::<f32>() - 0.5) * 5000.0;
        let pos = Point2::new(x, y);

        let w = rng.gen::<f32>() * 300.0 + 50.0;
        let h = 20.0; //rng.gen::<f32>() * 1.0 + 1.0;
        let size = Vector2::new(w, h);

        let angle = rng.gen::<f32>() * f32::consts::PI;

        wall::create(world, pos, size, angle);
    }
}

pub mod auth {
    //use physics::{Position, Velocity, AngularVelocity};
    //use repl;

    use super::*;

    pub fn create_state(world: &mut World) {
        super::create_state(world);

        // Just some stupid entities for initial testing

        /*repl::entity::auth::create(world, 0, "test", |builder| {
            builder.with(Position(Point2::new(0.0, -50.0)))
        });*/
        /*repl::entity::auth::create(world, 0, "test", |builder| {
            builder
                .with(Position(Point2::origin()))
                .with(Velocity(Vector2::new(1.0, 0.0)))
                .with(AngularVelocity(0.0))
        });
        repl::entity::auth::create(world, 0, "test", |builder| {
            builder
                .with(Position(Point2::new(0.0, 0.0)))
                .with(Velocity(Vector2::new(1.0, 0.0)))
                .with(AngularVelocity(0.1))
        });*/
        /*repl::entity::auth::create(world, 0, "test", |builder| {
            builder.with(Position(Point2::new(0.0, 50.0)))
        });*/
    }
}

pub mod view {
    use super::*;

    pub fn create_state(world: &mut World) {
        super::create_state(world);
    }
}

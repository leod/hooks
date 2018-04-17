use std::f32;

use nalgebra::{Point2, Vector2};
//use rand::{IsaacRng, Rng};
use specs::prelude::World;

use game::entity::test;
use game::entity::wall;
use physics::{AngularVelocity, Position, Velocity};
use repl;

fn create_wall_rect(world: &mut World, center: Point2<f32>, size: Vector2<f32>, d: f32) {
    wall::create(
        world,
        center - Vector2::new(0.0, size.y) / 2.0,
        Vector2::new(size.x + d, d),
        0.0,
    );
    wall::create(
        world,
        center + Vector2::new(0.0, size.y) / 2.0,
        Vector2::new(size.x + d, d),
        0.0,
    );
    wall::create(
        world,
        center - Vector2::new(size.x, 0.0) / 2.0,
        Vector2::new(size.y + d, d),
        f32::consts::PI / 2.0,
    );
    wall::create(
        world,
        center + Vector2::new(size.x, 0.0) / 2.0,
        Vector2::new(size.y + d, d),
        f32::consts::PI / 2.0,
    );
}

fn create_state(world: &mut World) {
    /*let n_walls = 200;
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
    }*/

    create_wall_rect(
        world,
        Point2::new(0.0, 0.0),
        Vector2::new(8000.0, 1500.0),
        200.0,
    );

    wall::create(
        world,
        Point2::new(0.0, -500.0),
        Vector2::new(1500.0, 20.0),
        0.0,
    );

    wall::create(
        world,
        Point2::new(-2000.0, 0.0),
        Vector2::new(1000.0, 20.0),
        f32::consts::PI / 2.0,
    );
}

pub mod auth {
    //use physics::{Position, Velocity, AngularVelocity};
    //use repl;

    use super::*;

    pub fn create_state(world: &mut World) {
        super::create_state(world);

        // Just some stupid entities for initial testing

        /*repl::entity::auth::create(world, 0, "test", |builder| {
            builder
                .with(Position(Point2::origin()))
                .with(Velocity(Vector2::new(100.0, 0.0)))
                .with(AngularVelocity(0.0))
                .with(test::auth::Test(2.5, 5.0))
        });
        repl::entity::auth::create(world, 0, "test", |builder| {
            builder
                .with(Position(Point2::new(0.0, 200.0)))
                .with(Velocity(Vector2::new(200.0, 0.0)))
                .with(AngularVelocity(0.1))
                .with(test::auth::Test(2.5, 5.0))
        });
        repl::entity::auth::create(world, 0, "test", |builder| {
            builder
                .with(Position(Point2::new(50.0, 400.0)))
                .with(Velocity(Vector2::new(50.0, 0.0)))
                .with(AngularVelocity(0.1))
                .with(test::auth::Test(2.5, 5.0))
        });*/
        repl::entity::auth::create(world, 0, "test", |builder| {
            builder
                .with(Position(Point2::new(100.0, 600.0)))
                .with(Velocity(Vector2::new(50.0, 0.0)))
                .with(AngularVelocity(5.0))
                .with(test::auth::Test(2.5, 5.0))
        });
        repl::entity::auth::create(world, 0, "test", |builder| {
            builder
                .with(Position(Point2::new(-50.0, 400.0)))
                .with(Velocity(Vector2::new(0.0, 0.0)))
                .with(AngularVelocity(5.0))
                .with(test::auth::Test(2.5, 5.0))
        });
        repl::entity::auth::create(world, 0, "test", |builder| {
            builder
                .with(Position(Point2::new(100.0, -200.0)))
                .with(Velocity(Vector2::new(200.0, 200.0)))
                .with(AngularVelocity(0.5))
                .with(test::auth::Test(2.5, 5.0))
        });
        repl::entity::auth::create(world, 0, "test", |builder| {
            builder
                .with(Position(Point2::new(100.0, 0.0)))
                .with(Velocity(Vector2::new(400.0, 0.0)))
                .with(AngularVelocity(0.5))
                .with(test::auth::Test(0.5, 1.0))
        });
    }
}

pub mod view {
    use super::*;

    pub fn create_state(world: &mut World) {
        super::create_state(world);
    }
}

use nalgebra::Vector2;

use defs::GameInfo;
use entity;
use game::ComponentType;
use physics::collision::{self, CollisionGroups, Cuboid, GeometricQueryType, ShapeHandle};
use physics::{AngularVelocity, Dynamic, InvAngularMass, InvMass, Orientation, Position, Velocity};
use registry::Registry;
use repl;

pub fn register(reg: &mut Registry) {
    repl::entity::register_class(
        reg,
        "test",
        &[ComponentType::Position, ComponentType::Orientation],
        |builder| {
            let shape = Cuboid::new(Vector2::new(100.0, 100.0));

            let mut groups = CollisionGroups::new();
            groups.set_membership(&[collision::GROUP_NEUTRAL]);
            groups.set_whitelist(&[collision::GROUP_PLAYER, collision::GROUP_PLAYER_ENTITY]);

            let query_type = GeometricQueryType::Contacts(0.0, 0.0);

            builder
                .with(collision::Shape(ShapeHandle::new(shape)))
                .with(collision::Object { groups, query_type })
        },
    );
}

pub mod auth {
    use specs::prelude::*;
    use specs::storage::BTreeStorage;

    use super::*;

    pub fn register(reg: &mut Registry) {
        super::register(reg);

        reg.component::<Test>();
        reg.tick_system(TickSys, "test", &[]);

        entity::add_ctor(reg, "test", |builder| {
            builder
                .with(Orientation(0.0))
                .with(Dynamic)
                .with(InvMass(1.0))
                .with(InvAngularMass(1.0))
        });
    }

    #[derive(Component)]
    #[storage(BTreeStorage)]
    pub struct Test(pub f32, pub f32);

    struct TickSys;

    #[derive(SystemData)]
    struct TickData<'a> {
        game_info: ReadExpect<'a, GameInfo>,
        test: WriteStorage<'a, Test>,
        position: WriteStorage<'a, Position>,
        orientation: WriteStorage<'a, Orientation>,
        velocity: WriteStorage<'a, Velocity>,
        angular_velocity: WriteStorage<'a, AngularVelocity>,
    }

    impl<'a> System<'a> for TickSys {
        type SystemData = TickData<'a>;

        fn run(&mut self, mut data: Self::SystemData) {
            let dt = data.game_info.tick_duration_secs();

            for (test, position, orientation, velocity, angular_velocity) in (
                &mut data.test,
                &mut data.position,
                &mut data.orientation,
                &mut data.velocity,
                &mut data.angular_velocity,
            ).join()
            {
                test.0 += dt;
                if test.0 >= test.1 {
                    test.0 = 0.0;
                    velocity.0 = -velocity.0;
                    angular_velocity.0 = -angular_velocity.0;
                }

                position.0 += velocity.0 * dt;
                orientation.0 += angular_velocity.0 * dt;
            }
        }
    }
}

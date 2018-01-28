use nalgebra::{zero, Vector2};
use specs::BTreeStorage;

use game::ComponentType;
use physics::{Dynamic, Orientation, Velocity};
use physics::collision::{self, CollisionGroups, Cuboid, GeometricQueryType, ShapeHandle};
use registry::Registry;
use repl::entity;

pub fn register(reg: &mut Registry) {
    reg.component::<Player>();

    entity::register_type(
        reg,
        "player",
        &[ComponentType::Position, ComponentType::Orientation],
        |builder| {
            let shape = Cuboid::new(Vector2::new(10.0, 10.0));
            let mut groups = CollisionGroups::new();
            groups.set_membership(&[collision::GROUP_PLAYER]);
            groups.set_whitelist(&[collision::GROUP_WALL]);
            let query_type = GeometricQueryType::Contacts(0.0);

            // TODO: Velocity (and Dynamic?) component should be added only for owners
            builder
                .with(Orientation(0.0))
                .with(Velocity(zero()))
                .with(Dynamic)
                .with(collision::Shape(ShapeHandle::new(shape)))
                .with(collision::CreateObject { groups, query_type })
        },
    );
}

#[derive(Component)]
#[component(BTreeStorage)]
struct Player;

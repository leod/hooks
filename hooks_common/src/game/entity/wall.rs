use nalgebra::{Point2, Vector2};
use specs::{DenseVecStorage, Entity, World};

use defs::{EntityId, INVALID_PLAYER_ID};
use registry::Registry;
use physics::{Orientation, Position};
use physics::collision::{self, CollisionGroups, Cuboid, GeometricQueryType, ShapeHandle};
use repl;
use game;

pub fn register(reg: &mut Registry) {
    reg.component::<Size>();

    repl::entity::register_class::<game::ComponentType>(reg, "wall", &[], |builder| builder);
}

#[derive(Component)]
#[component(DenseVecStorage)]
pub struct Size(pub Vector2<f32>);

pub fn create(
    world: &mut World,
    pos: Point2<f32>,
    size: Vector2<f32>,
    angle: f32,
) -> (EntityId, Entity) {
    assert!(size.x > 0.0);
    assert!(size.y > 0.0);

    let shape = Cuboid::new(size);

    let mut groups = CollisionGroups::new();
    groups.set_membership(&[collision::GROUP_WALL]);
    groups.set_whitelist(&[collision::GROUP_PLAYER, collision::GROUP_PLAYER_ENTITY]);

    let query_type = GeometricQueryType::Contacts(0.0, 0.0);

    let (entity_index, entity) =
        repl::entity::auth::create(world, INVALID_PLAYER_ID, "wall", |builder| {
            builder
                .with(Position(pos))
                .with(Orientation(angle))
                .with(Size(size * 2.0))
                .with(collision::Shape(ShapeHandle::new(shape)))
                .with(collision::Object { groups, query_type })
        });

    ((entity_index, INVALID_PLAYER_ID), entity)
}

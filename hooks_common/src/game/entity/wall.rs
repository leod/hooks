use nalgebra::{Point2, Vector2};
use specs::{DenseVecStorage, Entity, World};

use physics::{Orientation, Position};
use physics::collision::{self, CollisionGroups, Cuboid, GeometricQueryType, ShapeHandle};
use registry::Registry;

pub fn register(reg: &mut Registry) {
    reg.component::<Size>();
}

#[derive(Component)]
#[component(DenseVecStorage)]
pub struct Size(pub Vector2<f32>);

pub fn create(world: &mut World, pos: Point2<f32>, size: Vector2<f32>, angle: f32) -> Entity {
    assert!(size.x > 0.0);
    assert!(size.y > 0.0);

    let shape = Cuboid::new(size);

    let mut groups = CollisionGroups::new();
    groups.set_membership(&[collision::GROUP_WALL]);
    groups.set_whitelist(&[collision::GROUP_PLAYER]);

    let query_type = GeometricQueryType::Contacts(1000.0);

    world
        .create_entity()
        .with(Position { pos })
        .with(Orientation { angle })
        .with(Size(size * 2.0))
        .with(collision::Shape {
            shape: ShapeHandle::new(shape),
        })
        .with(collision::CreateObject { groups, query_type })
        .build()
}
use nalgebra::{zero, Point2, Vector2};
use specs::{BTreeStorage, World};

use defs::{EntityIndex, PlayerId};
use registry::Registry;
use physics::{Dynamic, Orientation, Position, Velocity};
use physics::collision::{self, CollisionGroups, Cuboid, GeometricQueryType, ShapeHandle};
use repl::{entity, player};
use game::ComponentType;

pub fn register(reg: &mut Registry) {
    reg.component::<Player>();
    reg.component::<Hook>();
    reg.component::<HookSegment>();

    entity::register_type(
        reg,
        "player",
        &[
            ComponentType::Position,
            ComponentType::Orientation,
            ComponentType::Player,
            ComponentType::Hook,
        ],
        |builder| {
            let shape = Cuboid::new(Vector2::new(10.0, 10.0));
            let mut groups = CollisionGroups::new();
            groups.set_membership(&[collision::GROUP_PLAYER]);
            groups.set_whitelist(&[collision::GROUP_WALL]);
            let query_type = GeometricQueryType::Contacts(0.0, 0.0);

            // TODO: Velocity (and Dynamic?) component should be added only for owners
            builder
                .with(Orientation(0.0))
                .with(Velocity(zero()))
                .with(Dynamic)
                .with(collision::Shape(ShapeHandle::new(shape)))
                .with(collision::CreateObject { groups, query_type })
                .with(Player)
        },
    );

    entity::register_type(
        reg,
        "hook_segment",
        &[
            ComponentType::Position,
            ComponentType::Orientation,
            ComponentType::HookSegment,
        ],
        |builder| {
            // TODO
            let shape = Cuboid::new(Vector2::new(1.0, 10.0));
            let mut groups = CollisionGroups::new();
            groups.set_membership(&[collision::GROUP_PLAYER]);
            groups.set_whitelist(&[collision::GROUP_WALL]);
            let query_type = GeometricQueryType::Contacts(0.0, 0.0);

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

#[derive(Component, PartialEq, Clone, BitStore)]
#[component(BTreeStorage)]
pub struct Player;

#[derive(Component, PartialEq, Clone, BitStore)]
#[component(BTreeStorage)]
pub struct Hook {
    pub is_active: bool,
    pub first_segment_index: EntityIndex,
}

#[derive(Component, PartialEq, Clone, BitStore)]
#[component(BTreeStorage)]
pub struct HookSegment {
    pub player_index: EntityIndex,
    pub is_last: bool,
}

const NUM_HOOK_SEGMENTS: usize = 10;

pub mod auth {
    use super::*;

    pub fn create(world: &mut World, owner: PlayerId, pos: Point2<f32>) {
        let player = player::get(world, owner).unwrap();
        let first_segment_index = player.next_entity_index(1);

        let (player_index, player_entity) =
            entity::auth::create(world, owner, "player", |builder| {
                let hook = Hook {
                    is_active: false,
                    first_segment_index,
                };

                builder.with(Position(pos)).with(hook)
            });

        for i in 0..NUM_HOOK_SEGMENTS {
            entity::auth::create(world, owner, "hook_segment", |builder| {
                let hook_segment = HookSegment {
                    player_index,
                    is_last: i == NUM_HOOK_SEGMENTS - 1,
                };

                builder.with(Position(pos)).with(hook_segment)
            });
        }
    }
}

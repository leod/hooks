use nalgebra::{zero, Point2, Vector2};
use specs::{BTreeStorage, Entities, Fetch, Join, ReadStorage, System, World, WriteStorage};

use defs::{EntityId, EntityIndex, PlayerId};
use registry::Registry;
use physics::{Dynamic, Joint, Joints, Mass, Orientation, Position, Velocity};
use physics::collision::{self, CollisionGroups, Cuboid, GeometricQueryType, ShapeHandle};
use repl::{self, entity, player, EntityMap};
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
                .with(Joints(Vec::new()))
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
            let shape = Cuboid::new(Vector2::new(4.0, 4.0));
            let mut groups = CollisionGroups::new();
            groups.set_membership(&[collision::GROUP_PLAYER]);
            groups.set_whitelist(&[collision::GROUP_WALL]);
            let query_type = GeometricQueryType::Contacts(0.0, 0.0);

            // TODO: Velocity (and Dynamic?) component should be added only for owners
            builder
                .with(Orientation(0.0))
                .with(Velocity(zero()))
                .with(Mass(1.0))
                .with(Dynamic)
                .with(Joints(Vec::new()))
                .with(collision::Shape(ShapeHandle::new(shape)))
                .with(collision::CreateObject { groups, query_type })
        },
    );

    // TODO: Check about when to best run hook system. Since it manages hook segment joints, would
    //       it be better if it runs before the physics simulation?
    // TODO: Should not register anything `auth` here.
    reg.tick_system(auth::HookSys, "hook", &[]);
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

const HOOK_JOINT: Joint = Joint {
    stiffness: 100.0,
    resting_length: 10.0,
};

pub fn hook_segment_indices(
    entity_map: &EntityMap,
    segments: &ReadStorage<HookSegment>,
    first_segment_id: EntityId,
) -> Result<Vec<EntityIndex>, repl::Error> {
    let (first_segment_owner, first_segment_index) = first_segment_id;

    let mut indices = Vec::new();
    let mut cur_index = first_segment_index;

    loop {
        let cur_id = (first_segment_owner, cur_index);
        let cur_entity = entity_map.try_id_to_entity(cur_id)?;

        if let Some(segment) = segments.get(cur_entity) {
            indices.push(cur_index);

            if !segment.is_last {
                cur_index += 1;
            } else {
                break;
            }
        } else {
            return Err(repl::Error::Replication(format!(
                "entity {:?} should be a hook segment",
                cur_index
            )));
        }
    }

    Ok(indices)
}

pub mod auth {
    use super::*;

    pub fn create(world: &mut World, owner: PlayerId, pos: Point2<f32>) {
        let player = player::get(world, owner).unwrap();
        let first_segment_index = player.next_entity_index(1);

        let (player_index, _) = entity::auth::create(world, owner, "player", |builder| {
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

    pub struct HookSys;

    impl<'a> System<'a> for HookSys {
        type SystemData = (
            Fetch<'a, EntityMap>,
            Entities<'a>,
            ReadStorage<'a, repl::Id>,
            ReadStorage<'a, Hook>,
            ReadStorage<'a, HookSegment>,
            WriteStorage<'a, Joints>,
        );

        fn run(
            &mut self,
            (entity_map, entities, repl_id, hook, segment, mut joints): Self::SystemData,
        ) {
            // Reset all joints of hook segments
            for (_, joints) in (&segment, &mut joints).join() {
                joints.0.clear();
            }

            // Add joints of hook segments
            for (hook_entity, &repl::Id((owner, _index)), hook) in
                (&*entities, &repl_id, &hook).join()
            {
                let segment_id = (owner, hook.first_segment_index);

                // TODO: Grr... repl unwrap. I think I understand monads now.
                let indices = hook_segment_indices(&entity_map, &segment, segment_id).unwrap();

                // Join player with first hook segment
                if let Some(&first_index) = indices.get(0) {
                    let first_entity = entity_map.try_id_to_entity((owner, first_index)).unwrap();

                    /*{
                        let entity_joints = joints.get_mut(hook_entity).unwrap();
                        entity_joints.0.clear(); // TODO: Where to clear joints?
                        entity_joints.0.push((first_entity, HOOK_JOINT.clone()));
                    }*/

                    joints
                        .get_mut(first_entity)
                        .unwrap()
                        .0
                        .push((hook_entity, HOOK_JOINT.clone()));
                }

                // Join successive hook segments
                for (&a_index, &b_index) in indices.iter().zip(indices.iter().skip(1)) {
                    let a_entity = entity_map.try_id_to_entity((owner, a_index)).unwrap();
                    let b_entity = entity_map.try_id_to_entity((owner, b_index)).unwrap();

                    joints
                        .get_mut(a_entity)
                        .unwrap()
                        .0
                        .push((b_entity, HOOK_JOINT.clone()));
                    joints
                        .get_mut(b_entity)
                        .unwrap()
                        .0
                        .push((a_entity, HOOK_JOINT.clone()));
                }
            }
        }
    }
}

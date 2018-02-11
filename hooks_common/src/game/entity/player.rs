use nalgebra::{zero, Point2, Vector2};
use specs::{BTreeStorage, Entities, Entity, EntityBuilder, Fetch, Join, ReadStorage, System,
            World, WriteStorage};

use defs::{EntityId, EntityIndex, PlayerId};
use registry::Registry;
use entity;
use physics::{interaction, Dynamic, Friction, Joint, Joints, Mass, Orientation, Position, Velocity};
use physics::collision::{self, CollisionGroups, Cuboid, GeometricQueryType, ShapeHandle};
use repl::{self, player, EntityMap};
use game::ComponentType;

pub fn register(reg: &mut Registry) {
    reg.component::<Player>();
    reg.component::<Hook>();
    reg.component::<HookSegment>();

    repl::entity::register_class(
        reg,
        "player",
        &[
            ComponentType::Position,
            ComponentType::Orientation,
            ComponentType::Player,
            ComponentType::Hook,
        ],
        build_player,
    );

    repl::entity::register_class(
        reg,
        "hook_segment",
        &[
            ComponentType::Position,
            ComponentType::Orientation,
            ComponentType::HookSegment,
        ],
        build_hook_segment,
    );

    interaction::add(reg, "hook_segment", "wall", hook_segment_wall_interaction);

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
    pub fixed: Option<(f32, f32)>,
}

const NUM_HOOK_SEGMENTS: usize = 10;

const HOOK_JOINT: Joint = Joint {
    stiffness: 200.0,
    resting_length: 10.0,
};

fn build_player(builder: EntityBuilder) -> EntityBuilder {
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
        .with(collision::Object { groups, query_type })
        .with(Player)
}

fn build_hook_segment(builder: EntityBuilder) -> EntityBuilder {
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
        .with(Friction)
        .with(Joints(Vec::new()))
        .with(collision::Shape(ShapeHandle::new(shape)))
        .with(collision::Object { groups, query_type })
}

fn hook_segment_wall_interaction(
    world: &World,
    segment_entity: Entity,
    _wall_entity: Entity,
    pos: Point2<f32>,
) {
    let mut segments = world.write::<HookSegment>();
    let segment = segments.get_mut(segment_entity).unwrap();

    if segment.is_last {
        segment.fixed = Some((pos.x, pos.y));
    }
}

/// Given the entity id of the first segment of a hook, returns a vector of the entities of all
/// segments belonging to this hook.
pub fn hook_segment_entities(
    entity_map: &EntityMap,
    segments: &ReadStorage<HookSegment>,
    first_segment_id: EntityId,
) -> Result<Vec<Entity>, repl::Error> {
    let (first_segment_owner, first_segment_index) = first_segment_id;

    let mut entities = Vec::new();
    let mut cur_index = first_segment_index;

    loop {
        let cur_id = (first_segment_owner, cur_index);
        let cur_entity = entity_map.try_id_to_entity(cur_id)?;

        if let Some(segment) = segments.get(cur_entity) {
            entities.push(cur_entity);

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

    Ok(entities)
}

pub mod auth {
    use super::*;

    pub fn create(world: &mut World, owner: PlayerId, pos: Point2<f32>) {
        let player = player::get(world, owner).unwrap();
        let first_segment_index = player.next_entity_index(1);

        let (player_index, _) = repl::entity::auth::create(world, owner, "player", |builder| {
            let hook = Hook {
                is_active: false,
                first_segment_index,
            };

            builder.with(Position(pos)).with(hook)
        });

        for i in 0..NUM_HOOK_SEGMENTS {
            repl::entity::auth::create(world, owner, "hook_segment", |builder| {
                let hook_segment = HookSegment {
                    player_index,
                    is_last: i == NUM_HOOK_SEGMENTS - 1,
                    fixed: None,
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
            ReadStorage<'a, HookSegment>,
            WriteStorage<'a, Hook>,
            WriteStorage<'a, Joints>,
            WriteStorage<'a, entity::Active>,
        );

        #[cfg_attr(rustfmt, rustfmt_skip)] // rustfmt bug
        fn run(
            &mut self,
            (
                entity_map,
                entities,
                repl_id,
                segment,
                mut hook,
                mut joints,
                mut active,
            ): Self::SystemData,
        ) {
            // Reset all joints of hook segments
            for (_, joints) in (&segment, &mut joints).join() {
                joints.0.clear();
            }

            for (hook_entity, &repl::Id((owner, _index)), hook) in
                (&*entities, &repl_id, &mut hook).join()
            {
                hook.is_active = false;

                let first_segment_id = (owner, hook.first_segment_index);

                // TODO: Grr... repl unwrap. I think I understand monads now?
                let segments =
                    hook_segment_entities(&entity_map, &segment, first_segment_id).unwrap();

                for &segment in &segments {
                    if hook.is_active {
                        active.insert(segment, entity::Active);
                    } else {
                        active.remove(segment);
                    }
                }

                // Join player with first hook segment
                if let Some(&first_segment) = segments.get(0) {
                    /*{
                        let entity_joints = joints.get_mut(hook_entity).unwrap();
                        entity_joints.0.clear(); // TODO: Where to clear joints?
                        entity_joints.0.push((first_entity, HOOK_JOINT.clone()));
                    }*/

                    joints
                        .get_mut(first_segment)
                        .unwrap()
                        .0
                        .push((hook_entity, HOOK_JOINT.clone()));
                }

                // Join successive hook segments
                for (&entity_a, &entity_b) in segments.iter().zip(segments.iter().skip(1)) {
                    joints
                        .get_mut(entity_a)
                        .unwrap()
                        .0
                        .push((entity_b, HOOK_JOINT.clone()));
                    joints
                        .get_mut(entity_b)
                        .unwrap()
                        .0
                        .push((entity_a, HOOK_JOINT.clone()));
                }
            }
        }
    }
}

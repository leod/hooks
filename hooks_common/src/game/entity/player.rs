use nalgebra::{zero, Point2, Vector2};
use specs::{BTreeStorage, Entities, Entity, EntityBuilder, Fetch, Join, ReadStorage, System,
            SystemData, World, WriteStorage};

use defs::{EntityId, EntityIndex, GameInfo, PlayerId, PlayerInput};
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
    //       Hook simulation should *not* run in every tick, but only if player input is given!
    // TODO: Should not register anything `auth` here.
    reg.tick_system(auth::HookSys, "hook", &[]);
}

/// Component that is attached whenever player input should be executed for an entity.
#[derive(Component, Clone, Debug)]
pub struct CurrentInput(pub PlayerInput);

#[derive(Component, PartialEq, Clone, Debug, BitStore)]
#[component(BTreeStorage)]
pub struct Player;

#[derive(PartialEq, Clone, Debug, BitStore)]
pub enum HookState {
    Inactive,
    Shooting { time_secs: f32 },
    //Contracting,
}

#[derive(Component, PartialEq, Clone, Debug, BitStore)]
#[component(BTreeStorage)]
pub struct Hook {
    pub first_segment_index: EntityIndex,
    pub state: HookState,
}

#[derive(Component, PartialEq, Clone, Debug, BitStore)]
#[component(BTreeStorage)]
pub struct HookSegment {
    pub player_index: EntityIndex,
    pub is_last: bool,
    pub fixed: Option<(f32, f32)>,
}

const HOOK_NUM_SEGMENTS: usize = 10;
const HOOK_MAX_SHOOT_TIME_SECS: f32 = 2.0;
const HOOK_SHOOT_SPEED: f32 = 2000.0;
const HOOK_JOINT: Joint = Joint {
    stiffness: 200.0,
    resting_length: 1.0,
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

pub fn shoot(world: &World, entity: Entity) {
    #[derive(SystemData)]
    struct Data<'a> {
        game_info: Fetch<'a, GameInfo>,
        entity_map: Fetch<'a, EntityMap>,

        repl_id: ReadStorage<'a, repl::Id>,
        orientation: ReadStorage<'a, Orientation>,
        segments: ReadStorage<'a, HookSegment>,

        position: WriteStorage<'a, Position>,
        velocity: WriteStorage<'a, Velocity>,
        hook: WriteStorage<'a, Hook>,
    }

    let mut data = Data::fetch(&world.res, 0);

    let _dt = data.game_info.tick_duration_secs() as f32;

    let entity_id = data.repl_id.get(entity).unwrap().0;
    let angle = data.orientation.get(entity).unwrap().0;
    let pos = data.position.get(entity).unwrap().0;
    let hook = data.hook.get_mut(entity).unwrap();

    if hook.state == HookState::Inactive {
        hook.state = HookState::Shooting { time_secs: 0.0 };

        let first_segment_id = (entity_id.0, hook.first_segment_index);

        // TODO: repl unwrap
        let segments =
            hook_segment_entities(&data.entity_map, &data.segments, first_segment_id).unwrap();

        for &segment in &segments {
            data.position.insert(segment, Position(pos));
            data.velocity.insert(segment, Velocity(zero()));
        }

        let xvel = Vector2::x_axis().unwrap() * angle.cos();
        let yvel = Vector2::y_axis().unwrap() * angle.sin();
        let vel = (xvel + yvel) * HOOK_SHOOT_SPEED;

        data.velocity
            .insert(*segments.last().unwrap(), Velocity(vel));
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
                first_segment_index,
                state: HookState::Inactive,
            };

            builder.with(Position(pos)).with(hook)
        });

        for i in 0..HOOK_NUM_SEGMENTS {
            repl::entity::auth::create(world, owner, "hook_segment", |builder| {
                let hook_segment = HookSegment {
                    player_index,
                    is_last: i == HOOK_NUM_SEGMENTS - 1,
                    fixed: None,
                };

                builder.with(Position(pos)).with(hook_segment)
            });
        }
    }

    pub struct HookSys;

    impl<'a> System<'a> for HookSys {
        type SystemData = (
            Fetch<'a, GameInfo>,
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
                game_info,
                entity_map,
                entities,
                repl_id,
                segment,
                mut hook,
                mut joints,
                mut active,
            ): Self::SystemData,
        ) {
            let dt = game_info.tick_duration_secs() as f32;

            // Reset all joints of hook segments
            for (_, joints) in (&segment, &mut joints).join() {
                joints.0.clear();
            }

            for (hook_entity, &repl::Id((owner, _index)), hook) in
                (&*entities, &repl_id, &mut hook).join()
            {
                hook.state = match hook.state {
                    HookState::Inactive => HookState::Inactive,
                    HookState::Shooting { time_secs } => {
                        let new_time_secs = time_secs + dt;

                        if new_time_secs >= HOOK_MAX_SHOOT_TIME_SECS {
                            HookState::Inactive
                        } else {
                            HookState::Shooting { time_secs: new_time_secs }
                        }
                    }
                };

                let first_segment_id = (owner, hook.first_segment_index);

                // TODO: Grr... repl unwrap. I think I understand monads now?
                let segments =
                    hook_segment_entities(&entity_map, &segment, first_segment_id).unwrap();

                for &segment in &segments {
                    if hook.state != HookState::Inactive {
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

use std::ops::Deref;

use nalgebra::{norm, zero, Point2, Rotation2, Vector2};
use specs::{BTreeStorage, Entities, Entity, EntityBuilder, Fetch, Join, MaskedStorage,
            NullStorage, ReadStorage, RunNow, Storage, System, World, WriteStorage};

use defs::{EntityId, EntityIndex, GameInfo, PlayerId, PlayerInput};
use registry::Registry;
use entity::Active;
use physics::{interaction, Dynamic, Friction, Joint, Joints, Mass, Orientation, Position, Velocity};
use physics::collision::{self, CollisionGroups, Cuboid, GeometricQueryType, ShapeHandle};
use repl::{self, player, EntityMap};
use game::ComponentType;

pub fn register(reg: &mut Registry) {
    reg.component::<CurrentInput>();
    reg.component::<Player>();
    reg.component::<Hook>();
    reg.component::<HookSegment>();
    reg.component::<ActivateHookSegment>();
    reg.component::<DeactivateHookSegment>();

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
            ComponentType::Active,
            ComponentType::Position,
            ComponentType::Orientation,
            ComponentType::HookSegment,
        ],
        build_hook_segment,
    );

    interaction::set(
        reg,
        "player",
        "wall",
        Some(interaction::Action::PreventOverlap),
        None,
    );
    interaction::set(
        reg,
        "hook_segment",
        "wall",
        Some(interaction::Action::PreventOverlap),
        Some(hook_segment_wall_interaction),
    );
    interaction::set(
        reg,
        "hook_segment",
        "player",
        None,
        Some(hook_segment_player_interaction),
    );
}

/// Component that is attached whenever player input should be executed for an entity.
#[derive(Component, Clone, Debug)]
#[component(BTreeStorage)]
pub struct CurrentInput(pub PlayerInput);

#[derive(Component, PartialEq, Clone, Debug, BitStore)]
#[component(BTreeStorage)]
pub struct Player;

#[derive(PartialEq, Clone, Debug, BitStore)]
pub enum HookState {
    Inactive,
    Shooting { time_secs: f32 },
    Contracting,
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
    // TODO: `player_index` and `is_last` could be inferred in theory, but it wouldn't look pretty
    pub player_index: EntityIndex,
    pub is_last: bool,
    pub fixed: Option<(f32, f32)>,
}

#[derive(Component, PartialEq, Clone, Debug)]
#[component(BTreeStorage)]
struct ActivateHookSegment {
    position: Point2<f32>,
    velocity: Vector2<f32>,
}

#[derive(Component, PartialEq, Clone, Debug, Default)]
#[component(NullStorage)]
struct DeactivateHookSegment;

const MOVE_ACCEL: f32 = 300.0;
const MOVE_SPEED: f32 = 100.0;

const HOOK_NUM_SEGMENTS: usize = 10;
const HOOK_MAX_SHOOT_TIME_SECS: f32 = 2.0;
const HOOK_SHOOT_SPEED: f32 = 300.0;
const HOOK_JOINT: Joint = Joint {
    stiffness: 50.0,
    resting_length: 30.0,
};
const HOOK_JOINT_2: Joint = Joint {
    stiffness: 100.0,
    resting_length: 60.0,
};
const HOOK_JOINT_CONTRACT: Joint = Joint {
    stiffness: 100.0,
    resting_length: 0.0,
};

pub fn run_input(world: &mut World, entity: Entity, input: &PlayerInput) {
    world
        .write::<CurrentInput>()
        .insert(entity, CurrentInput(input.clone()));

    InputSys.run_now(&world.res);

    world.write::<CurrentInput>().clear();
}

/// Given the entity id of the first segment of a hook, returns a vector of the entities of all
/// segments belonging to this hook.
pub fn hook_segment_entities<D>(
    entity_map: &EntityMap,
    segments: &Storage<HookSegment, D>,
    first_segment_id: EntityId,
) -> Result<Vec<Entity>, repl::Error>
where
    D: Deref<Target = MaskedStorage<HookSegment>>,
{
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

    assert!(entities.len() == HOOK_NUM_SEGMENTS);

    Ok(entities)
}

pub fn active_hook_segment_entities<D1, D2>(
    entity_map: &EntityMap,
    active: &Storage<Active, D1>,
    segments: &Storage<HookSegment, D2>,
    first_segment_id: EntityId,
) -> Result<Vec<Entity>, repl::Error>
where
    D1: Deref<Target = MaskedStorage<Active>>,
    D2: Deref<Target = MaskedStorage<HookSegment>>,
{
    let entities = hook_segment_entities(entity_map, segments, first_segment_id)?;

    Ok(entities
        .into_iter()
        .filter(|&entity| active.get(entity).unwrap().0)
        .collect())
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
            let (_, entity) = repl::entity::auth::create(world, owner, "hook_segment", |builder| {
                let hook_segment = HookSegment {
                    player_index,
                    is_last: i == HOOK_NUM_SEGMENTS - 1,
                    fixed: None,
                };

                builder.with(Position(pos)).with(hook_segment)
            });

            // We create the hook segments in an inactive state
            world.write::<Active>().insert(entity, Active(false));
        }
    }
}

fn build_player(builder: EntityBuilder) -> EntityBuilder {
    let shape = Cuboid::new(Vector2::new(10.0, 10.0));

    let mut groups = CollisionGroups::new();
    groups.set_membership(&[collision::GROUP_PLAYER]);
    groups.set_whitelist(&[collision::GROUP_WALL, collision::GROUP_PLAYER_ENTITY]);

    let query_type = GeometricQueryType::Contacts(0.0, 0.0);

    // TODO: Velocity (and Dynamic?) component should be added only for owners
    builder
        .with(Orientation(0.0))
        .with(Velocity(zero()))
        .with(Mass(10.0))
        .with(Dynamic)
        .with(Friction(15.0))
        .with(Joints(Vec::new()))
        .with(collision::Shape(ShapeHandle::new(shape)))
        .with(collision::Object { groups, query_type })
        .with(Player)
}

fn build_hook_segment(builder: EntityBuilder) -> EntityBuilder {
    // TODO
    let shape = Cuboid::new(Vector2::new(4.0, 4.0));

    let mut groups = CollisionGroups::new();
    groups.set_membership(&[collision::GROUP_PLAYER_ENTITY]);
    groups.set_whitelist(&[collision::GROUP_WALL, collision::GROUP_PLAYER]);

    let query_type = GeometricQueryType::Contacts(0.0, 0.0);

    // TODO: Velocity (and Dynamic?) component should be added only for owners
    builder
        .with(Orientation(0.0))
        .with(Velocity(zero()))
        .with(Mass(1.0))
        .with(Dynamic)
        .with(Friction(1.0))
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

    if segment.is_last && segment.fixed.is_none() {
        segment.fixed = Some((pos.x, pos.y));
    }
}

fn hook_segment_player_interaction(
    world: &World,
    segment_entity: Entity,
    player_entity: Entity,
    _pos: Point2<f32>,
) {
    debug!("yo");

    let mut hooks = world.write::<Hook>();
    let hook = hooks.get_mut(player_entity).unwrap();

    if hook.state == HookState::Contracting {
        // Eat up the first segment if it comes close enough to our mouth.

        let &repl::Id((owner, _)) = world.read::<repl::Id>().get(player_entity).unwrap();
        let first_segment_id = (owner, hook.first_segment_index);

        let active_segments = active_hook_segment_entities(
            &world.read_resource::<EntityMap>(),
            &world.read::<Active>(),
            &world.read::<HookSegment>(),
            first_segment_id,
        ).unwrap();

        if active_segments.first().cloned() == Some(segment_entity) {
            // Yummy!
            let mut actives = world.write::<Active>();
            let active = actives.get_mut(segment_entity).unwrap();
            active.0 = false;
        }
    }
}

#[derive(SystemData)]
struct InputData<'a> {
    game_info: Fetch<'a, GameInfo>,
    entity_map: Fetch<'a, EntityMap>,
    entities: Entities<'a>,

    input: ReadStorage<'a, CurrentInput>,
    repl_id: ReadStorage<'a, repl::Id>,

    active: WriteStorage<'a, Active>,
    dynamic: WriteStorage<'a, Dynamic>,

    position: WriteStorage<'a, Position>,
    velocity: WriteStorage<'a, Velocity>,
    orientation: WriteStorage<'a, Orientation>,
    joints: WriteStorage<'a, Joints>,

    hook: WriteStorage<'a, Hook>,
    segment: WriteStorage<'a, HookSegment>,
    activate_segment: WriteStorage<'a, ActivateHookSegment>,
    deactivate_segment: WriteStorage<'a, DeactivateHookSegment>,
}

struct InputSys;

impl<'a> System<'a> for InputSys {
    type SystemData = InputData<'a>;

    fn run(&mut self, mut data: InputData<'a>) {
        let dt = data.game_info.tick_duration_secs() as f32;

        /*
         * Movement
         */
        for (input, orientation, velocity) in
            (&data.input, &mut data.orientation, &mut data.velocity).join()
        {
            if input.0.rot_angle != orientation.0 {
                // TODO: Only mutate if changed
                orientation.0 = input.0.rot_angle;
            }

            let forward = Rotation2::new(orientation.0).matrix() * Vector2::new(1.0, 0.0);

            if input.0.move_forward {
                velocity.0 += forward * MOVE_ACCEL * dt;
            //velocity.0 = forward * MOVE_SPEED;
            } else if input.0.move_backward {
                velocity.0 -= forward * MOVE_SPEED * dt;
            } else {
                //velocity.0 = Vector2::new(0.0, 0.0);
            }
        }

        /*
         * Update hook
         */
        for (entity, input, repl_id, orientation, position, velocity, hook) in (
            &*data.entities,
            &data.input,
            &data.repl_id,
            &data.orientation,
            &data.position,
            &data.velocity,
            &mut data.hook,
        ).join()
        {
            /*
             * Reset all joints
             */
            data.joints.get_mut(entity).unwrap().0.clear();

            for (segment_id, _, joints) in (&data.repl_id, &data.segment, &mut data.joints).join() {
                if (segment_id.0).0 == (repl_id.0).0 {
                    joints.0.clear();
                }
            }

            // TODO: repl unwrap
            let first_segment_id = ((repl_id.0).0, hook.first_segment_index);
            let segments =
                hook_segment_entities(&data.entity_map, &data.segment, first_segment_id).unwrap();

            /*
             * Throw the hook
             */
            if input.0.shoot_one {
                if hook.state == HookState::Inactive {
                    hook.state = HookState::Shooting { time_secs: 0.0 };

                    let last_segment = *segments.last().unwrap();

                    let xvel = Vector2::x_axis().unwrap() * orientation.0.cos();
                    let yvel = Vector2::y_axis().unwrap() * orientation.0.sin();
                    let segment_velocity = (xvel + yvel) * HOOK_SHOOT_SPEED;

                    data.activate_segment.insert(
                        last_segment,
                        ActivateHookSegment {
                            position: position.0,
                            velocity: segment_velocity + velocity.0,
                        },
                    );
                }
            }

            /*
             * Update hook state
             */
            let active_segments = active_hook_segment_entities(
                &data.entity_map,
                &data.active,
                &data.segment,
                first_segment_id,
            ).unwrap();

            match hook.state.clone() {
                HookState::Inactive => {}
                HookState::Shooting { time_secs } => {
                    let new_time_secs = time_secs; //+ dt;
                    if new_time_secs >= HOOK_MAX_SHOOT_TIME_SECS {
                    } else if !active_segments.is_empty() {
                        hook.state = HookState::Shooting {
                            time_secs: new_time_secs,
                        };

                        let last_segment = *active_segments.last().unwrap();
                        let is_last_fixed = data.segment.get(last_segment).unwrap().fixed.is_some();

                        if !input.0.shoot_one || is_last_fixed {
                            hook.state = HookState::Contracting;
                        } else {
                            let first_segment = *active_segments.first().unwrap();
                            let first_position = data.position.get(first_segment).unwrap().0.coords;

                            // Activate new segment when the newest one is far enough from us
                            let distance = norm(&(first_position - position.0.coords));
                            if distance >= HOOK_JOINT.resting_length {
                                if active_segments.len() < segments.len() {
                                    let next_segment =
                                        segments[segments.len() - (active_segments.len() + 1)];

                                    let xvel = Vector2::x_axis().unwrap() * orientation.0.cos();
                                    let yvel = Vector2::y_axis().unwrap() * orientation.0.sin();
                                    let segment_velocity = (xvel + yvel) * HOOK_SHOOT_SPEED;

                                    data.activate_segment.insert(
                                        next_segment,
                                        ActivateHookSegment {
                                            position: position.0,
                                            velocity: segment_velocity + velocity.0,
                                        },
                                    );
                                } else {
                                    hook.state = HookState::Contracting;
                                }
                            }
                        }
                    }
                }
                HookState::Contracting => {
                    if active_segments.is_empty() {
                        hook.state = HookState::Inactive;
                    }
                }
            };

            /*
             * Join player with first hook segments
             */
            if let Some(&first_segment) = active_segments.get(0) {
                let joint = match hook.state {
                    HookState::Contracting => HOOK_JOINT_CONTRACT.clone(),
                    _ => HOOK_JOINT.clone(),
                };

                let is_fixed = data.segment
                    .get(*active_segments.last().unwrap())
                    .unwrap()
                    .fixed
                    .is_some();

                if hook.state != HookState::Contracting || is_fixed {
                    let entity_joints = data.joints.get_mut(entity).unwrap();
                    entity_joints.0.clear();
                    entity_joints.0.push((first_segment, joint.clone()));
                }

                data.joints
                    .get_mut(first_segment)
                    .unwrap()
                    .0
                    .push((entity, joint.clone()));

                if hook.state != HookState::Contracting {
                    // Join with second hook segment
                    if let Some(&second_segment) = active_segments.get(1) {
                        {
                            let entity_joints = data.joints.get_mut(entity).unwrap();
                            entity_joints.0.clear();
                            entity_joints.0.push((second_segment, HOOK_JOINT_2.clone()));
                        }

                        data.joints
                            .get_mut(second_segment)
                            .unwrap()
                            .0
                            .push((entity, HOOK_JOINT_2.clone()));
                    }
                }
            }

            /*
             * Join successive hook segments
             */
            let active_segment_pairs = active_segments.iter().zip(active_segments.iter().skip(1));
            for (&entity_a, &entity_b) in active_segment_pairs {
                data.joints
                    .get_mut(entity_a)
                    .unwrap()
                    .0
                    .push((entity_b, HOOK_JOINT.clone()));
                data.joints
                    .get_mut(entity_b)
                    .unwrap()
                    .0
                    .push((entity_a, HOOK_JOINT.clone()));
            }

            /*
             * Join hook segments with a distance of two
             */
            let active_segment_pairs = active_segments.iter().zip(active_segments.iter().skip(2));
            for (&entity_a, &entity_b) in active_segment_pairs {
                data.joints
                    .get_mut(entity_a)
                    .unwrap()
                    .0
                    .push((entity_b, HOOK_JOINT_2.clone()));
                data.joints
                    .get_mut(entity_b)
                    .unwrap()
                    .0
                    .push((entity_a, HOOK_JOINT_2.clone()));
            }
        }

        /*
         * Activate new hook segments
         */
        for (activate, active, position, velocity, segment) in (
            &data.activate_segment,
            &mut data.active,
            &mut data.position,
            &mut data.velocity,
            &mut data.segment,
        ).join()
        {
            active.0 = true;
            position.0 = activate.position;
            velocity.0 = activate.velocity;
            segment.fixed = None;
        }

        data.activate_segment.clear();

        /*
         * Maintain dynamic flag: a fixed hook segment should not move in the physics simulation
         */
        for (entity, segment, position) in
            (&*data.entities, &data.segment, &mut data.position).join()
        {
            if let Some((x, y)) = segment.fixed {
                position.0 = Point2::new(x, y);
                data.dynamic.remove(entity);
            } else {
                data.dynamic.insert(entity, Dynamic);
            }
        }

        /*
         * Deactivate hook segments
         */
        for (_, active) in (&data.deactivate_segment, &mut data.active).join() {
            active.0 = false;
        }

        data.deactivate_segment.clear();
    }
}

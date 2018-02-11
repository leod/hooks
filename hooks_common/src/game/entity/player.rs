use std::ops::Deref;

use nalgebra::{zero, Point2, Rotation2, Vector2};
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

    interaction::add(reg, "hook_segment", "wall", hook_segment_wall_interaction);
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

#[derive(Component, PartialEq, Clone, Debug)]
#[component(BTreeStorage)]
struct ActivateHookSegment {
    position: Point2<f32>,
    velocity: Vector2<f32>,
}

#[derive(Component, PartialEq, Clone, Debug, Default)]
#[component(NullStorage)]
struct DeactivateHookSegment;

const MOVE_SPEED: f32 = 100.0;

const HOOK_NUM_SEGMENTS: usize = 10;
const HOOK_MAX_SHOOT_TIME_SECS: f32 = 2.0;
const HOOK_SHOOT_SPEED: f32 = 2000.0;
const HOOK_JOINT: Joint = Joint {
    stiffness: 200.0,
    resting_length: 1.0,
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

#[derive(SystemData)]
struct InputData<'a> {
    game_info: Fetch<'a, GameInfo>,
    entity_map: Fetch<'a, EntityMap>,
    entities: Entities<'a>,

    input: ReadStorage<'a, CurrentInput>,
    repl_id: ReadStorage<'a, repl::Id>,

    active: WriteStorage<'a, Active>,
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
            // TODO: Only mutate if changed

            if input.0.rot_angle != orientation.0 {
                orientation.0 = input.0.rot_angle;
            }

            let forward = Rotation2::new(orientation.0).matrix() * Vector2::new(1.0, 0.0);

            if input.0.move_forward {
                velocity.0 = forward * MOVE_SPEED;
            } else if input.0.move_backward {
                velocity.0 = -forward * MOVE_SPEED;
            } else {
                velocity.0 = Vector2::new(0.0, 0.0);
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
             * Reset all joints of hook segments
             */
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

                    for &segment in &segments {
                        let segment_velocity = if data.segment.get(segment).unwrap().is_last {
                            let xvel = Vector2::x_axis().unwrap() * orientation.0.cos();
                            let yvel = Vector2::y_axis().unwrap() * orientation.0.sin();
                            (xvel + yvel) * HOOK_SHOOT_SPEED
                        } else {
                            zero()
                        };

                        data.activate_segment.insert(
                            segment,
                            ActivateHookSegment {
                                position: position.0,
                                velocity: segment_velocity + velocity.0,
                            },
                        );
                    }
                }
            }

            hook.state = match hook.state {
                HookState::Inactive => HookState::Inactive,
                HookState::Shooting { time_secs } => {
                    let new_time_secs = time_secs + dt;

                    if new_time_secs >= HOOK_MAX_SHOOT_TIME_SECS {
                        for &segment in &segments {
                            data.deactivate_segment
                                .insert(segment, DeactivateHookSegment);
                        }

                        HookState::Inactive
                    } else {
                        HookState::Shooting {
                            time_secs: new_time_secs,
                        }
                    }
                }
            };

            // Join player with first hook segment
            if let Some(&first_segment) = segments.get(0) {
                /*{
                    let entity_joints = joints.get_mut(hook_entity).unwrap();
                    entity_joints.0.clear(); // TODO: Where to clear joints?
                    entity_joints.0.push((first_entity, HOOK_JOINT.clone()));
                }*/

                data.joints
                    .get_mut(first_segment)
                    .unwrap()
                    .0
                    .push((entity, HOOK_JOINT.clone()));
            }

            // Join successive hook segments
            for (&entity_a, &entity_b) in segments.iter().zip(segments.iter().skip(1)) {
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
         * Deactivate hook segments
         */
        for (_, active) in (&data.deactivate_segment, &mut data.active).join() {
            active.0 = false;
        }

        data.deactivate_segment.clear();
    }
}

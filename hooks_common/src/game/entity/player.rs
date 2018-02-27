use std::ops::Deref;

use nalgebra::{norm, zero, Point2, Rotation2, Vector2};
use specs::{BTreeStorage, Entities, Entity, EntityBuilder, Fetch, FetchMut, Join, MaskedStorage,
            NullStorage, ReadStorage, RunNow, Storage, System, World, WriteStorage};

use defs::{EntityId, EntityIndex, GameInfo, PlayerId, PlayerInput};
use registry::Registry;
use entity::Active;
use physics::interaction;
use physics::constraint::{self, Constraint};
use physics::{AngularVelocity, Dynamic, Friction, InvAngularMass, InvMass, Orientation, Position,
              Velocity};
use physics::collision::{self, CollisionGroups, Cuboid, GeometricQueryType, ShapeHandle};
use physics::sim::Constraints;
use repl::{self, player, EntityMap};
use game::ComponentType;

// NOTE: This module is heavily work-in-progress and is mostly used for prototyping hook mechanics.
//       Implementation of hook interactions are currently hairy since it involves lots of
//       unwrapping entities/components. Since this kind of interaction will occur more frequently
//       in the game, we need to find a better way to do this.

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
        Some(interaction::Action::PreventOverlap {
            rotate_a: false,
            rotate_b: false,
        }),
        None,
    );
    interaction::set(
        reg,
        "hook_segment",
        "wall",
        Some(interaction::Action::PreventOverlap {
            rotate_a: false,
            rotate_b: false,
        }),
        //None,
        Some(hook_segment_wall_interaction),
    );
    /*interaction::set(
        reg,
        "hook_segment",
        "player",
        None,
        Some(hook_segment_player_interaction),
    );*/
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
    Contracting { lunch_timer: f32 },
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
    orientation: f32,
}

#[derive(Component, PartialEq, Clone, Debug, Default)]
#[component(NullStorage)]
struct DeactivateHookSegment;

const MOVE_ACCEL: f32 = 300.0;
const MOVE_SPEED: f32 = 100.0;

pub const HOOK_NUM_SEGMENTS: usize = 15;
pub const HOOK_SEGMENT_LENGTH: f32 = 30.0;
const HOOK_MAX_SHOOT_TIME_SECS: f32 = 2.0;
const HOOK_SHOOT_SPEED: f32 = 500.0;
const HOOK_LUNCH_TIME_SECS: f32 = 0.1;
const HOOK_LUNCH_RADIUS: f32 = 5.0;

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
    let shape = Cuboid::new(Vector2::new(20.0, 20.0));

    let mut groups = CollisionGroups::new();
    groups.set_membership(&[collision::GROUP_PLAYER]);
    groups.set_whitelist(&[collision::GROUP_WALL, collision::GROUP_PLAYER_ENTITY]);

    let query_type = GeometricQueryType::Contacts(0.0, 0.0);

    // TODO: Velocity (and Dynamic?) component should be added only for owners
    builder
        .with(Orientation(0.0))
        .with(Velocity(zero()))
        .with(AngularVelocity(0.0))
        .with(InvMass(1.0 / 200.0))
        .with(InvAngularMass(1.0 / 10.0))
        .with(Dynamic)
        .with(Friction(200.0 * 100.0))
        .with(collision::Shape(ShapeHandle::new(shape)))
        .with(collision::Object { groups, query_type })
        .with(Player)
}

fn build_hook_segment(builder: EntityBuilder) -> EntityBuilder {
    // TODO
    let shape = Cuboid::new(Vector2::new(HOOK_SEGMENT_LENGTH / 2.0, 3.0));

    let mut groups = CollisionGroups::new();
    groups.set_membership(&[collision::GROUP_PLAYER_ENTITY]);
    groups.set_whitelist(&[collision::GROUP_WALL, collision::GROUP_PLAYER]);

    let query_type = GeometricQueryType::Contacts(0.0, 0.0);

    // TODO: Velocity (and Dynamic?) component should be added only for owners
    builder
        .with(Orientation(0.0))
        .with(Velocity(zero()))
        .with(AngularVelocity(0.0))
        .with(InvMass(1.0 / 5.0))
        .with(InvAngularMass(
            12.0 / (5.0 * (HOOK_SEGMENT_LENGTH.powi(2) + 9.0)),
        ))
        .with(Dynamic)
        .with(Friction(5.0))
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

#[derive(SystemData)]
struct InputData<'a> {
    game_info: Fetch<'a, GameInfo>,
    entity_map: Fetch<'a, EntityMap>,
    entities: Entities<'a>,
    constraints: FetchMut<'a, Constraints>,

    input: ReadStorage<'a, CurrentInput>,
    repl_id: ReadStorage<'a, repl::Id>,

    active: WriteStorage<'a, Active>,
    dynamic: WriteStorage<'a, Dynamic>,

    position: WriteStorage<'a, Position>,
    velocity: WriteStorage<'a, Velocity>,
    orientation: WriteStorage<'a, Orientation>,

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

        // Movement
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

        // Update hook
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
            // TODO: repl unwrap
            let first_segment_id = ((repl_id.0).0, hook.first_segment_index);
            let segments =
                hook_segment_entities(&data.entity_map, &data.segment, first_segment_id).unwrap();

            let active_segments = active_hook_segment_entities(
                &data.entity_map,
                &data.active,
                &data.segment,
                first_segment_id,
            ).unwrap();

            // Update hook state
            match hook.state.clone() {
                HookState::Inactive => {
                    if input.0.shoot_one {
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
                                orientation: orientation.0,
                            },
                        );
                    }
                }
                HookState::Shooting { time_secs } => {
                    let new_time_secs = time_secs; //+ dt;
                    if new_time_secs >= HOOK_MAX_SHOOT_TIME_SECS {
                    } else if let (Some(&first_segment), Some(&last_segment)) =
                        (active_segments.first(), active_segments.last())
                    {
                        hook.state = HookState::Shooting {
                            time_secs: new_time_secs,
                        };

                        let is_last_fixed = data.segment.get(last_segment).unwrap().fixed.is_some();

                        if !input.0.shoot_one || is_last_fixed {
                            hook.state = HookState::Contracting { lunch_timer: 0.0 };
                        } else {
                            let first_position = data.position.get(first_segment).unwrap().0.coords;

                            // Activate new segment when the newest one is far enough from us
                            let distance = norm(&(first_position - position.0.coords));
                            if distance >= HOOK_SEGMENT_LENGTH / 2.0 {
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
                                            orientation: orientation.0,
                                        },
                                    );
                                } else {
                                    hook.state = HookState::Contracting { lunch_timer: 0.0 };
                                }
                            }
                        }
                    }
                }
                HookState::Contracting { lunch_timer } => {
                    if !input.0.shoot_one {
                        if let Some(&last_segment) = active_segments.last() {
                            data.segment.get_mut(last_segment).unwrap().fixed = None;
                        }
                    }

                    // Join player with first hook segments
                    // activate spook hier
                    if let (Some(&first_segment), Some(&last_segment)) =
                        (active_segments.first(), active_segments.last())
                    {
                        let new_lunch_timer = (lunch_timer + dt).min(HOOK_LUNCH_TIME_SECS);
                        hook.state = HookState::Contracting {
                            lunch_timer: new_lunch_timer,
                        };

                        let segment_p = data.position.get(first_segment).unwrap().0;
                        let segment_rot = Rotation2::new(
                            data.orientation.get(first_segment).unwrap().0,
                        ).matrix()
                            .clone();
                        let segment_attach_p = segment_rot *
                            Point2::new(-HOOK_SEGMENT_LENGTH / 2.0, 0.0) +
                            segment_p.coords;

                        let target_distance =
                            (1.0 - new_lunch_timer / HOOK_LUNCH_TIME_SECS) * HOOK_SEGMENT_LENGTH;
                        let cur_distance = norm(&(segment_attach_p - position.0));

                        //debug!("target {} cur {}", target_distance, cur_distance);

                        // Eat up the first segment if it comes close enough to our mouth.
                        if cur_distance < HOOK_LUNCH_RADIUS {
                            // Yummy!
                            let segment_active = data.active.get_mut(first_segment).unwrap();
                            segment_active.0 = false;

                            hook.state = HookState::Contracting { lunch_timer: 0.0 };
                        } else {
                            let constraint_distance = cur_distance.min(target_distance);

                            let is_last_fixed =
                                data.segment.get(last_segment).unwrap().fixed.is_some();

                            let joint_def = constraint::Def::Joint {
                                distance: constraint_distance,
                                p_object_a: Point2::origin(),
                                p_object_b: Point2::new(-HOOK_SEGMENT_LENGTH / 2.0, 0.0),
                            };
                            //let angle_def = constraint::Def::Angle { angle: 0.0 };

                            let joint_constraint = Constraint {
                                entity_a: entity,
                                entity_b: first_segment,
                                vars_a: constraint::Vars {
                                    p: is_last_fixed || true,
                                    angle: false,
                                },
                                vars_b: constraint::Vars {
                                    p: true,
                                    angle: true,
                                },
                                def: joint_def,
                            };
                            data.constraints.add(joint_constraint);

                            /*let angle_constraint = Constraint {
                                entity_a: entity,
                                entity_b: first_segment,
                                vars_a: constraint::Vars {
                                    p: false,
                                    angle: false,
                                },
                                vars_b: constraint::Vars {
                                    p: false,
                                    angle: true,
                                },
                                def: angle_def,
                            };
                            data.constraints.add(angle_constraint);*/
                        }
                    } else {
                        hook.state = HookState::Inactive;
                    }
                }
            };

            // Join successive hook segments
            let active_segment_pairs = active_segments.iter().zip(active_segments.iter().skip(1));
            for (&entity_a, &entity_b) in active_segment_pairs {
                let joint_def = constraint::Def::Joint {
                    distance: 0.0,
                    p_object_a: Point2::new(HOOK_SEGMENT_LENGTH / 2.0, 0.0),
                    p_object_b: Point2::new(-HOOK_SEGMENT_LENGTH / 2.0, 0.0),
                };
                let angle_def = constraint::Def::Angle { angle: 0.0 };

                /*let sum_def = constraint::Def::Sum(
                    Box::new(joint_def),
                    Box::new(angle_def),
                );
                let sum_constraint = Constraint {
                    entity_a,
                    entity_b,
                    vars_a: constraint::Vars {
                        p: true,
                        angle: true,
                    },
                    vars_b: constraint::Vars {
                        p: true,
                        angle: true,
                    },
                    def: sum_def,
                };
                data.constraints.add(sum_constraint);*/

                let joint_constraint = Constraint {
                    entity_a,
                    entity_b,
                    vars_a: constraint::Vars {
                        p: true,
                        angle: true,
                    },
                    vars_b: constraint::Vars {
                        p: true,
                        angle: true,
                    },
                    def: joint_def,
                };
                let angle_constraint = Constraint {
                    entity_a,
                    entity_b,
                    vars_a: constraint::Vars {
                        p: false,
                        angle: true,
                    },
                    vars_b: constraint::Vars {
                        p: false,
                        angle: true,
                    },
                    def: angle_def,
                };
                data.constraints.add(joint_constraint);
                data.constraints.add(angle_constraint);
            }
        }

        // Activate new hook segments
        for (activate, active, position, velocity, orientation, segment) in (
            &data.activate_segment,
            &mut data.active,
            &mut data.position,
            &mut data.velocity,
            &mut data.orientation,
            &mut data.segment,
        ).join()
        {
            active.0 = true;
            position.0 = activate.position;
            velocity.0 = activate.velocity;
            orientation.0 = activate.orientation;
            segment.fixed = None;
        }

        data.activate_segment.clear();

        // Maintain dynamic flag: a fixed hook segment should not move in the physics simulation
        // TODO: Update with constraints!
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

        // Deactivate hook segments
        for (_, active) in (&data.deactivate_segment, &mut data.active).join() {
            active.0 = false;
        }

        data.deactivate_segment.clear();
    }
}

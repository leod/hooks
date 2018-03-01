use nalgebra::{norm, zero, Point2, Vector2};
use specs::{Fetch, FetchMut, ReadStorage, World, WriteStorage};

use registry::Registry;
use defs::{EntityId, INVALID_ENTITY_ID};
use repl;
use physics::interaction;
use physics::constraint;
use physics::collision::{self, CollisionGroups, Cuboid, GeometricQueryType, ShapeHandle};
use physics::{AngularVelocity, Dynamic, Friction, InvAngularMass, InvMass, Orientation, Position,
              Velocity};
use game::ComponentType;

pub fn register(reg: &mut Registry) {
    reg.component::<Def>();
    reg.component::<SegmentDef>();
    reg.component::<State>();
    reg.component::<CurrentInput>();

    repl::entity::register_class(
        reg,
        "hook_segment",
        &[
            ComponentType::HookSegmentDef,
            ComponentType::Position,
            ComponentType::Orientation,
        ],
        build_segment,
    );

    // The first hook segment is special in the sense that it can attach to other entities. Other
    // than that, it has the same properties as a normal hook segment.
    repl::entity::register_class(
        reg,
        "first_hook_segment",
        &[
            ComponentType::HookSegmentDef,
            ComponentType::Position,
            ComponentType::Orientation,
        ],
        build_segment,
    );

    // The hook entity only carries state and is otherwise invisible
    repl::entity::register_class(
        reg,
        "hook",
        &[ComponentType::HookDef, ComponentType::HookState],
        |builder| builder,
    );

    interaction::set(
        reg,
        "hook_segment",
        "wall",
        Some(interaction::Action::PreventOverlap {
            rotate_a: false,
            rotate_b: false,
        }),
        None,
    );
    interaction::set(
        reg,
        "first_hook_segment",
        "wall",
        Some(interaction::Action::PreventOverlap {
            rotate_a: false,
            rotate_b: false,
        }),
        Some(first_segment_wall_interaction),
    );
}

pub const NUM_SEGMENTS: usize = 20;
pub const SEGMENT_LENGTH: f32 = 30.0;
const MAX_SHOOT_TIME_SECS: f32 = 2.0;
const SHOOT_SPEED: f32 = 600.0;
const LUNCH_TIME_SECS: f32 = 0.05;
const LUNCH_RADIUS: f32 = 5.0;

/// Definition of a hook. This should not change in the entity's lifetime. Thus, we can store
/// a relatively large amount of data here, since it is only sent once to each client due to delta
/// serialization.
///
/// NOTE: Here, we store `EntityId`s of related entities. There is some redundancy in this, since a
///       `EntityId` also contains the `PlayerId` of the entity owner. As we assume that every hook
///       belongs to exactly one player, this means we are sending redundant information. However,
///       the increased ease of use wins here for now.
/// TODO: Could save some comparisons by allowing to declare components as constant during an
///       entity's lifetime.
#[derive(Component, PartialEq, Clone, Debug, BitStore)]
pub struct Def {
    /// Every hook belongs to one entity. For shooting the hook, the entity is assumed to have
    /// `Position`, `Orientation` and `Velocity` components.
    pub owner: EntityId,

    /// When a hook entity is created, all of the segments are immediately created as well. Their
    /// ids are stored here. In `State`, we replicate the number of hook segments that are
    /// currently active, if any, corresponding to the first elements of this array.
    pub segments: [EntityId; HOOK_NUM_SEGMENTS],
}

/// Definition of a hook segment. Again, this should not change in the entity's lifetime.
#[derive(Component, PartialEq, Clone, Debug, BitStore)]
pub struct SegmentDef {
    /// Every hook segment belongs to one hook.
    pub hook: EntityId,
}

/// The mode a hook can be in. This is replicated and expected to change frequently.
#[derive(PartialEq, Clone, Debug, BitStore)]
pub enum Mode {
    /// The hook is expanding, shooting out new segments.
    Shooting,
    /// The hook is contracting.
    Contracting {
        /// Timer for when we expect to munch up the next hook segment.
        lunch_timer: f32,

        /// While contracting, the hook can be attached to another entity. We store the
        /// object-space coordinates in terms of the other entity.
        fixed: Option<(EntityId, (f32, f32))>,
    },
}

/// The dynamic state of an active hook.
#[derive(PartialEq, Clone, Debug, BitStore)]
pub struct ActiveState {
    pub num_active_segments: u8,
    pub mode: Mode,
}

/// The dynamic state of a hook.
#[derive(Component, PartialEq, Clone, Debug, BitStore)]
pub struct State(pub Option<ActiveState>);

/// Input for simulating a hook.
#[derive(Component, Clone, Debug)]
#[component(BTreeStorage)]
pub struct CurrentInput {
    pub shoot: bool,
}

fn build_segment(builder: EntityBuilder) -> EntityBuilder {
    // TODO
    let shape = Cuboid::new(Vector2::new(SEGMENT_LENGTH / 2.0, 3.0));

    let mut groups = CollisionGroups::new();
    groups.set_membership(&[collision::GROUP_PLAYER_ENTITY]);
    groups.set_whitelist(&[collision::GROUP_WALL]);

    let query_type = GeometricQueryType::Contacts(0.0, 0.0);

    // TODO: Velocity (and Dynamic?) component should be added only for owners
    builder
        .with(Position(zero()))
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

/// Only the server can create hooks, so the following is nested in an `auth` module.
pub mod auth {
    use super::*;

    pub fn create(world: &mut World, owner: EntityId) -> (EntityId, Entity) {
        assert!(
            world
                .read_resource::<repl::EntityMap>()
                .get_id_to_entity(owner)
                .is_some(),
            "hook owner entity does not exist"
        );

        // Create hook
        let (id, entity) =
            repl::entity::auth::create(world, owner.0, "hook", |builder| builder.with(State(None)));

        // Create hook segments
        let mut segments = [INVALID_ENTITY_ID; NUM_SEGMENTS];
        for i in 0..NUM_SEGMENTS {
            let (segment_id, _) =
                repl::entity::auth::create(world, owner.0, "hook_segment", |builder| {
                    let def = SegmentDef { hook: id };
                    builder.with(def);
                });
            segments[i] = segment_id;
        }

        // Now that we have the IDs of all the segments, attach the definition to the hook
        world
            .write::<Def>()
            .insert(hook_entity, Def { owner, segments });

        (id, entity)
    }
}

fn first_segment_wall_interaction(
    world: &World,
    segment_entity: Entity,
    wall_entity: Entity,
    _p_object_segment: Point2<f32>,
    p_object_wall: Point2<f32>,
) {
    let wall_id = {
        let repl_ids = world.read::<repl::Id>();

        // Can unwrap since every wall entity must have a `repl::Id`.
        repl_ids.get(wall_entity).unwrap().0;
    };

    // Get the corresponding hook entity.
    // TODO: Validate that received entities have the components specified by their class.
    let segment_def = world.read::<SegmentDef>().get(segment_entity).unwrap();

    // TODO: Repl unwrap: server could send faulty hook id in segment definition
    let hook_entity = repl::get_id_to_entity(world, segment_def.hook).unwrap();

    // TODO: Validate that received entities have the components specified by their class.
    let mut state = world.write::<State>().get(hook_entity).unwrap();

    let mut active_state = state
        .0
        .as_ref()
        .expect("got segment wall interaction, but hook is inactive");

    // Only attach if we are not attached yet
    match active_state.mode.clone() {
        Mode::Shooting { .. } => {
            active_state.mode = Mode::Contracting {
                lunch_timer: 0.0,
                fixed: Some((wall_id, (p_object_wall.x, p_object_wall.y))),
            }
        }
        Mode::Contracting {
            lunch_timer,
            fixed: None,
        } => {
            active_state.mode = Mode::Contracting {
                lunch_timer,
                fixed: Some((wall_id, (p_object_wall.x, p_object_wall.y))),
            }
        }
        _ => {}
    }
}

#[derive(SystemData)]
struct InputData<'a> {
    game_info: Fetch<'a, GameInfo>,
    entity_map: Fetch<'a, repl::EntityMap>,
    entities: Entities<'a>,

    constraints: FetchMut<'a, Constraints>,

    input: ReadStorage<'a, CurrentInput>,
    repl_id: ReadStorage<'a, repl::Id>,
    hook_def: ReadStorage<'a, Def>,
    segment_def: ReadStorage<'a, SegmentDef>,

    active: WriteStorage<'a, Active>,
    position: WriteStorage<'a, Position>,
    velocity: WriteStorage<'a, Velocity>,
    orientation: WriteStorage<'a, Orientation>,
    hook_state: WriteStorage<'a, State>,
}

pub fn run_input_sys(world: &World) -> Result<repl::Error, ()> {
    let dt = data.game_info.tick_duration_secs() as f32;

    let data = InputData::fetch(&world.res, 0);

    // Update all hooks that currently have some input attached to them
    for (input, hook_def, hook_state) in (&data.input, &data.hook_def, &data.hook_state).join() {
        // Stalk our owner
        let owner_entity = data.entity_map.try_id_to_entity(hook_def.owner)?;
        let owner_pos = data.position
            .get(owner_entity)
            .ok_or(repl::MissingComponent(hook_def.owner, "Position"))?
            .0;
        let owner_angle = data.orientation
            .get(owner_entity)
            .ok_or(repl::MissingComponent(hook_def.owner, "Orientation"))?
            .0;
        let owner_vel = data.velocity
            .get(owner_entity)
            .ok_or(repl::MissingComponent(hook_def.owner, "Velocity"))?
            .0;

        // Look up our segments
        let mut segment_entities = [owner_entity; NUM_SEGMENTS];
        for i in 0..NUM_SEGMENTS {
            segment_entities[i] = data.entity_map.try_id_to_entity(hook_def.segments[i])?;
        }

        // Update hook state
        match hook_state.0.clone() {
            Some(ActiveState {
                num_active_segments,
                mode: Mode::Shooting,
            }) => {
                if !input.shoot {
                    // Start contracting the hook
                    hook_state.0 = Some(ActiveState {
                        num_active_segments,
                        mode: Mode::Contracting {
                            lunch_timer: 0.0,
                            fixed: None,
                        },
                    });
                } else {
                    // Keep on shooting
                    let activate_next = num_active_segments == 0 || {
                        // Activate next segment when the last one is far enough from us
                        let last_entity = segment_entities[num_active_segments - 1];
                        let last_pos = data.position
                            .get(last_entity)
                            .ok_or(repl::MissingComponent(
                                hook_def.segments[num_active_segments - 1],
                                "Position",
                            ))?
                            .0;
                        let distance = norm(&(last_pos - owner_pos));
                        distance >= SEGMENT_LENGTH / 2.0
                    };

                    let join_index = if activate_next {
                        if num_active_segments + 1 < NUM_SEGMENTS {
                            let segment_id = hook_def.segments[num_active_segments + 1];
                            let next_segment = data.entity_map
                                .try_id_to_entity(segment_id)
                                .ok_or(repl::InvalidEntity(hook_def.owner))?;

                            let vel = Vector2::new(owner_angle.cos(), owner_angle.cos())
                                * HOOK_SHOOT_SPEED;

                            data.position.insert(next_segment, Position(owner_pos));
                            data.orientation
                                .insert(next_segment, Orientation(owner_angle));
                            data.velocity.insert(next_segment, owner_vel + vel);

                            num_active_segments
                        } else {
                            // No segments left, switch to contracting
                            hook_state.0 = Some(ActiveState {
                                num_active_segments,
                                mode: Mode::Contracting {
                                    lunch_timer: 0.0,
                                    fixed: None,
                                },
                            });

                            num_active_segments - 1
                        }
                    } else {
                        num_active_segments - 1
                    };

                    // Join player with last hook segment if it gets too far away
                    let join_entity = segment_entities[join_index];
                    let join_id = hook_def.segments[join_index];
                    let last_pos = data.position
                        .get(last_entity)
                        .ok_or(repl::MissingComponent(join_id, "Position"))?
                        .0;
                    let last_angle = data.orientation
                        .get(last_entity)
                        .ok_or(repl::MissingComponent(join_id, "Orientation"))?
                        .0;
                    let last_rot = Rotation2::new(last_angle).matrix().clone();
                    let last_attach_pos =
                        last_rot * Point2::new(-SEGMENT_LENGTH / 2.0, 0.0) + last_pos.coords;

                    let target_distance = 0.0;
                    let cur_distance = norm(&(lsat_attach_pos - owner_pos));

                    if cur_distance > SEGMENT_LENGTH / 2.0 {
                        let constraint_distance = cur_distance.min(target_distance);

                        let joint_def = constraint::Def::Joint {
                            distance: constraint_distance,
                            p_object_a: Point2::origin(),
                            p_object_b: Point2::new(-SEGMENT_LENGTH / 2.0, 0.0),
                        };

                        let joint_constraint = Constraint {
                            def: joint_def,
                            stiffness: 1.0,
                            entity_a: owner_entity,
                            entity_b: join_entity,
                            vars_a: constraint::Vars {
                                p: true,
                                angle: false,
                            },
                            vars_b: constraint::Vars {
                                p: true,
                                angle: true,
                            },
                        };
                        data.constraints.add(joint_constraint);
                    }
                }
            }
            Some(ActiveState {
                num_active_segments,
                mode: Mode::Contracting { lunch_timer, fixed },
            }) => {
                // Are we done contracting?
                if num_active_segments == 0 {
                    hook_state.0 = None;
                } else {
                    let new_fixed = if !input.shoot { None } else { fixed };
                    let new_lunch_timer = (lunch_timer + dt).min(LUNCH_TIME_SECS);

                    // Fix last hook segment to the entity it has been attached to
                    if let Some((fix_entity_id, (fix_x, fix_y))) = new_fixed {
                        if let Some(fix_entity) = data.entity_map.get_id_to_entity(fix_entity_id) {
                            let joint_def = constraint::Def::Joint {
                                distance: 0.0,
                                p_object_a: Point2::new(SEGMENT_LENGTH / 2.0, 0.0),
                                p_object_b: Point2::new(fix_x, fix_y),
                            };
                            let joint_constraint = Constraint {
                                def: joint_def,
                                stiffness: 1.0,
                                entity_a: last_entity,
                                entity_b: fix_entity,
                                vars_a: constraint::Vars {
                                    p: true,
                                    angle: true,
                                },
                                vars_b: constraint::Vars {
                                    p: false,
                                    angle: false,
                                },
                            };
                            data.constraints.add(joint_constraint);
                        } else {
                            warn!("hook attached to dead entity {:?}", fix_entity_id);
                        }
                    }

                    // Eat up the last segment if it comes close enough to our mouth.
                    let last_entity = segment_entities[num_active_segments - 1];
                    let last_pos = data.position
                        .get(last_entity)
                        .ok_or(repl::MissingComponent(join_id, "Position"))?
                        .0;
                    let last_angle = data.orientation
                        .get(last_entity)
                        .ok_or(repl::MissingComponent(join_id, "Orientation"))?
                        .0;
                    let last_rot = Rotation2::new(last_angle).matrix().clone();
                    let last_attach_pos =
                        last_rot * Point2::new(-SEGMENT_LENGTH / 2.0, 0.0) + last_pos.coords;

                    let cur_distance = norm(&(last_attach_pos - owner_pos));

                    let (new_lunch_timer, new_num_active_segments) =
                        if cur_distance < HOOK_LUNCH_RADIUS {
                            // Yummy!
                            let segment_active = data.active.get_mut(first_segment).unwrap();
                            segment_active.0 = false;

                            (0.0, num_active_segments - 1)
                        } else {
                            (new_lunch_timer, num_active_segments)
                        };

                    if new_num_active_segments == 0 {
                        // We are done eating...
                        hook_state.0 = None;
                    } else {
                        // Constrain ourself to the last segment, getting closer over time
                        let last_entity = segment_entities[new_num_active_segments - 1];
                        let last_pos = data.position
                            .get(last_entity)
                            .ok_or(repl::MissingComponent(join_id, "Position"))?
                            .0;
                        let last_angle = data.orientation
                            .get(last_entity)
                            .ok_or(repl::MissingComponent(join_id, "Orientation"))?
                            .0;
                        let last_rot = Rotation2::new(last_angle).matrix().clone();
                        let last_attach_pos =
                            last_rot * Point2::new(-SEGMENT_LENGTH / 2.0, 0.0) + last_pos.coords;

                        let target_distance =
                            (1.0 - new_lunch_timer / LUNCH_TIME_SECS) * SEGMENT_LENGTH;
                        let constraint_distance = cur_distance.min(target_distance);

                        let joint_def = constraint::Def::Joint {
                            distance: constraint_distance,
                            p_object_a: Point2::origin(),
                            p_object_b: Point2::new(-HOOK_SEGMENT_LENGTH / 2.0, 0.0),
                        };
                        let joint_constraint = Constraint {
                            def: joint_def,
                            stiffness: 1.0,
                            entity_a: entity,
                            entity_b: last_entity,
                            vars_a: constraint::Vars {
                                p: true,
                                angle: false,
                            },
                            vars_b: constraint::Vars {
                                p: true,
                                angle: true,
                            },
                        };
                        data.constraints.add(joint_constraint);

                        let angle_def = constraint::Def::Angle { angle: 0.0 };
                        let angle_constraint = Constraint {
                            def: angle_def,
                            stiffness: 1.0,
                            entity_a: owner_entity,
                            entity_b: last_entity,
                            vars_a: constraint::Vars {
                                p: false,
                                angle: false,
                            },
                            vars_b: constraint::Vars {
                                p: false,
                                angle: true,
                            },
                        };
                        data.constraints.add(angle_constraint);
                    }
                }
            }
            None => {
                if input.shoot {
                    // Start shooting the hook
                    hook_state.0 = Some(ActiveState {
                        num_active_segments: 0,
                        mode: Mode::Shooting,
                    });
                }
            }
        }

        // Join successive hook segments
        if let &Some(ActiveState {
            num_active_segments,
            ..
        }) = &hook_state.0
        {
            assert!(num_active_segments > 0);

            for i in 0..num_active_segments - 1 {
                let entity_a = segment_entities[i];
                let entity_b = segment_entities[i + 1];

                let joint_def = constraint::Def::Joint {
                    distance: 0.0,
                    p_object_a: Point2::new(HOOK_SEGMENT_LENGTH / 2.0, 0.0),
                    p_object_b: Point2::new(-HOOK_SEGMENT_LENGTH / 2.0, 0.0),
                };
                let angle_def = constraint::Def::Angle { angle: 0.0 };

                let joint_constraint = Constraint {
                    def: joint_def,
                    stiffness: 1.0,
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
                };
                //let j = active_segments.len() - i - 1;
                //let stiffness = (j as f32 / HOOK_NUM_SEGMENTS as f32).powi(2);
                let stiffness = 0.7;
                let angle_constraint = Constraint {
                    def: angle_def,
                    stiffness: stiffness,
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
                };
                data.constraints.add(joint_constraint);
                data.constraints.add(angle_constraint);
            }
        }
    }
}

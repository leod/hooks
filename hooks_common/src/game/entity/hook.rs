use nalgebra::{norm, zero, Point2, Rotation2, Vector2};
use specs::{Entity, EntityBuilder, Fetch, FetchMut, Join, ReadStorage, SystemData, World,
            WriteStorage};

use registry::Registry;
use defs::{EntityId, GameInfo, INVALID_ENTITY_ID};
use event::{self, Event};
use entity::Active;
use repl;
use physics::{AngularVelocity, Dynamic, Friction, InvAngularMass, InvMass, Orientation, Position,
              Velocity};
use physics::interaction;
use physics::sim::Constraints;
use physics::constraint::{self, Constraint};
use physics::collision::{self, CollisionGroups, Cuboid, GeometricQueryType, ShapeHandle};
use game::ComponentType;

pub fn register(reg: &mut Registry) {
    reg.component::<Def>();
    reg.component::<SegmentDef>();
    reg.component::<State>();
    reg.component::<CurrentInput>();

    reg.event::<FixedEvent>();

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
//const MAX_SHOOT_TIME_SECS: f32 = 2.0;
const SHOOT_SPEED: f32 = 600.0;
const LUNCH_TIME_SECS: f32 = 0.05;
const LUNCH_RADIUS: f32 = 5.0;

/// This event is emitted when a hook attaches at some point. This is meant to be used for
/// visualization purposes.
#[derive(Debug, Clone, BitStore)]
pub struct FixedEvent {
    pub pos: [f32; 2],
    pub vel: [f32; 2],
}

impl Event for FixedEvent {
    fn class(&self) -> event::Class {
        event::Class::Order
    }
}

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
    /// Different hook colors for drawing.
    pub index: u32,

    /// Every hook belongs to one entity. For shooting the hook, the entity is assumed to have
    /// `Position`, `Orientation` and `Velocity` components.
    pub owner: EntityId,

    /// When a hook entity is created, all of the segments are immediately created as well. Their
    /// ids are stored here. In `State`, we replicate the number of hook segments that are
    /// currently active, if any, corresponding to the first elements of this array.
    pub segments: [EntityId; NUM_SEGMENTS],
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
        fixed: Option<(EntityId, [f32; 2])>,
    },
}

/// The dynamic state of an active hook.
#[derive(PartialEq, Clone, Debug, BitStore)]
pub struct ActiveState {
    pub num_active_segments: u8,
    pub want_fix: bool,
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
        .with(Position(Point2::origin()))
        .with(Orientation(0.0))
        .with(Velocity(zero()))
        .with(AngularVelocity(0.0))
        .with(InvMass(1.0 / 5.0))
        .with(InvAngularMass(
            12.0 / (5.0 * (SEGMENT_LENGTH.powi(2) + 9.0)),
        ))
        .with(Dynamic)
        .with(Friction(5.0))
        .with(collision::Shape(ShapeHandle::new(shape)))
        .with(collision::Object { groups, query_type })
}

/// Only the server can create hooks, so the following is nested in an `auth` module.
pub mod auth {
    use super::*;

    pub fn create(world: &mut World, owner: EntityId, index: u32) -> (EntityId, Entity) {
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
            // The first hook segment is special because it can attach to entities
            let class = if i == 0 {
                "first_hook_segment"
            } else {
                "hook_segment"
            };

            let (segment_id, _) = repl::entity::auth::create(world, owner.0, class, |builder| {
                builder.with(SegmentDef { hook: id })
            });
            segments[i] = segment_id;
        }

        // Now that we have the IDs of all the segments, attach the definition to the hook
        world.write::<Def>().insert(
            entity,
            Def {
                index,
                owner,
                segments,
            },
        );

        (id, entity)
    }
}

fn first_segment_wall_interaction(
    world: &World,
    segment_info: &interaction::EntityInfo,
    wall_info: &interaction::EntityInfo,
    pos: Point2<f32>,
    normal: Vector2<f32>,
) {
    let wall_id = {
        let repl_ids = world.read::<repl::Id>();

        // Can unwrap since every wall entity must have a `repl::Id`.
        repl_ids.get(wall_info.entity).unwrap().0
    };

    // Get the corresponding hook entity.
    // TODO: Validate that received entities have the components specified by their class.
    let hook_id = world
        .read::<SegmentDef>()
        .get(segment_info.entity)
        .unwrap()
        .hook;

    // TODO: Repl unwrap: server could send faulty hook id in segment definition
    let hook_entity = repl::get_id_to_entity(world, hook_id).unwrap();

    // TODO: Validate that received entities have the components specified by their class.
    let mut state_storage = world.write::<State>();
    let state = state_storage.get_mut(hook_entity).unwrap();

    let active_state = state
        .0
        .as_mut()
        .expect("got segment wall interaction, but hook is inactive");

    if !active_state.want_fix {
        return;
    }

    // Only attach if we are not attached yet
    let fixed = match active_state.mode.clone() {
        Mode::Shooting { .. } => {
            active_state.mode = Mode::Contracting {
                lunch_timer: 0.0,
                fixed: Some((wall_id, wall_info.pos_object.coords.into())),
            };
            true
        }
        Mode::Contracting {
            lunch_timer,
            fixed: None,
        } => {
            active_state.mode = Mode::Contracting {
                lunch_timer,
                fixed: Some((wall_id, wall_info.pos_object.coords.into())),
            };
            true
        }
        _ => false,
    };

    if fixed {
        let event = FixedEvent {
            pos: pos.coords.into(),
            vel: normal.into(),
        };
        world.write_resource::<event::Sink>().push(event);
    }
}

#[derive(SystemData)]
struct InputData<'a> {
    game_info: Fetch<'a, GameInfo>,
    entity_map: Fetch<'a, repl::EntityMap>,

    constraints: FetchMut<'a, Constraints>,

    input: ReadStorage<'a, CurrentInput>,
    hook_def: ReadStorage<'a, Def>,

    active: WriteStorage<'a, Active>,
    position: WriteStorage<'a, Position>,
    velocity: WriteStorage<'a, Velocity>,
    orientation: WriteStorage<'a, Orientation>,
    hook_state: WriteStorage<'a, State>,
}

pub fn run_input_sys(world: &World) -> Result<(), repl::Error> {
    let mut data = InputData::fetch(&world.res, 0);

    let dt = data.game_info.tick_duration_secs();

    // Update all hooks that currently have some input attached to them
    for (input, hook_def, hook_state) in (&data.input, &data.hook_def, &mut data.hook_state).join()
    {
        //debug!("{:?}", hook_state);

        // Stalk our owner
        let owner_entity = data.entity_map.try_id_to_entity(hook_def.owner)?;
        let owner_pos = data.position
            .get(owner_entity)
            .ok_or(repl::Error::MissingComponent(hook_def.owner, "Position"))?
            .0;
        let owner_angle = data.orientation
            .get(owner_entity)
            .ok_or(repl::Error::MissingComponent(hook_def.owner, "Orientation"))?
            .0;
        let owner_vel = data.velocity
            .get(owner_entity)
            .ok_or(repl::Error::MissingComponent(hook_def.owner, "Velocity"))?
            .0;

        // Look up our segments
        let mut segment_entities = [owner_entity; NUM_SEGMENTS];
        for i in 0..NUM_SEGMENTS {
            segment_entities[i] = data.entity_map.try_id_to_entity(hook_def.segments[i])?;
        }
        // TODO: num_active_segments could be out of bounds

        // Update hook state
        match hook_state.0.clone() {
            Some(ActiveState {
                num_active_segments,
                mode: Mode::Shooting,
                ..
            }) => {
                if !input.shoot {
                    // Start contracting the hook
                    hook_state.0 = Some(ActiveState {
                        num_active_segments,
                        want_fix: false,
                        mode: Mode::Contracting {
                            lunch_timer: 0.0,
                            fixed: None,
                        },
                    });
                } else {
                    let num_active_segments = num_active_segments as usize;
                    let new_want_fix = input.shoot;

                    // Keep on shooting
                    let activate_next = num_active_segments == 0 || {
                        // Activate next segment when the last one is far enough from us
                        let last_entity = segment_entities[num_active_segments - 1];
                        let last_pos = data.position
                            .get(last_entity)
                            .ok_or(repl::Error::MissingComponent(
                                hook_def.segments[num_active_segments - 1],
                                "Position",
                            ))?
                            .0;
                        let distance = norm(&(last_pos - owner_pos));
                        distance >= SEGMENT_LENGTH / 2.0
                    };

                    let join_index = if activate_next {
                        if num_active_segments + 1 < NUM_SEGMENTS {
                            let segment_index = num_active_segments;
                            let next_segment = segment_entities[segment_index];

                            let vel =
                                Vector2::new(owner_angle.cos(), owner_angle.sin()) * SHOOT_SPEED;

                            data.position.insert(next_segment, Position(owner_pos));
                            data.orientation
                                .insert(next_segment, Orientation(owner_angle));
                            data.velocity
                                .insert(next_segment, Velocity(owner_vel + vel));

                            hook_state.0 = Some(ActiveState {
                                num_active_segments: num_active_segments as u8 + 1,
                                want_fix: new_want_fix,
                                mode: Mode::Shooting,
                            });

                            // Join with this new last segment
                            num_active_segments
                        } else {
                            // No segments left, switch to contracting
                            hook_state.0 = Some(ActiveState {
                                num_active_segments: num_active_segments as u8,
                                want_fix: new_want_fix,
                                mode: Mode::Contracting {
                                    lunch_timer: 0.0,
                                    fixed: None,
                                },
                            });

                            // Still join with last segment if necessary
                            num_active_segments - 1
                        }
                    } else {
                        hook_state.0 = Some(ActiveState {
                            num_active_segments: num_active_segments as u8,
                            want_fix: new_want_fix,
                            mode: Mode::Shooting,
                        });

                        num_active_segments - 1
                    };

                    // Join player with last hook segment if it gets too far away
                    let join_entity = segment_entities[join_index];
                    let join_id = hook_def.segments[join_index];
                    let join_pos = data.position
                        .get(join_entity)
                        .ok_or(repl::Error::MissingComponent(join_id, "Position"))?
                        .0;
                    let join_angle = data.orientation
                        .get(join_entity)
                        .ok_or(repl::Error::MissingComponent(join_id, "Orientation"))?
                        .0;
                    let join_rot = Rotation2::new(join_angle).matrix().clone();
                    let join_attach_pos =
                        join_rot * Point2::new(-SEGMENT_LENGTH / 2.0, 0.0) + join_pos.coords;

                    let target_distance = 0.0;
                    let cur_distance = norm(&(join_attach_pos - owner_pos));

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
                ..
            }) => {
                let new_want_fix = input.shoot;

                // Are we done contracting?
                if num_active_segments == 0 {
                    hook_state.0 = None;
                } else {
                    let new_fixed = if !input.shoot { None } else { fixed };
                    let new_lunch_timer = (lunch_timer + dt).min(LUNCH_TIME_SECS);

                    // Fix last hook segment to the entity it has been attached to
                    if let Some((fix_entity_id, fix_pos_object)) = new_fixed {
                        if let Some(fix_entity) = data.entity_map.get_id_to_entity(fix_entity_id) {
                            let joint_def = constraint::Def::Joint {
                                distance: 0.0,
                                p_object_a: Point2::new(SEGMENT_LENGTH / 2.0, 0.0),
                                p_object_b: Point2::new(fix_pos_object[0], fix_pos_object[1]),
                            };
                            let joint_constraint = Constraint {
                                def: joint_def,
                                stiffness: 1.0,
                                entity_a: segment_entities[0],
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
                    let last_index = num_active_segments as usize - 1;
                    let last_id = hook_def.segments[last_index];
                    let last_entity = segment_entities[last_index];
                    let last_pos = data.position
                        .get(last_entity)
                        .ok_or(repl::Error::MissingComponent(last_id, "Position"))?
                        .0;
                    let last_angle = data.orientation
                        .get(last_entity)
                        .ok_or(repl::Error::MissingComponent(last_id, "Orientation"))?
                        .0;
                    let last_rot = Rotation2::new(last_angle).matrix().clone();
                    let last_attach_pos =
                        last_rot * Point2::new(-SEGMENT_LENGTH / 2.0, 0.0) + last_pos.coords;

                    let cur_distance = norm(&(last_attach_pos - owner_pos));

                    let (new_lunch_timer, new_num_active_segments) = if cur_distance < LUNCH_RADIUS
                    {
                        // Yummy!
                        (0.0, num_active_segments - 1)
                    } else {
                        (new_lunch_timer, num_active_segments)
                    };

                    if new_num_active_segments == 0 {
                        // We are done eating...
                        hook_state.0 = None;
                    } else {
                        hook_state.0 = Some(ActiveState {
                            num_active_segments: new_num_active_segments as u8,
                            want_fix: new_want_fix,
                            mode: Mode::Contracting {
                                lunch_timer: new_lunch_timer,
                                fixed: new_fixed,
                            },
                        });

                        // Constrain ourself to the last segment, getting closer over time
                        let last_index = new_num_active_segments as usize - 1;
                        let last_id = hook_def.segments[last_index];
                        let last_entity = segment_entities[last_index];
                        let last_pos = data.position
                            .get(last_entity)
                            .ok_or(repl::Error::MissingComponent(last_id, "Position"))?
                            .0;
                        let last_angle = data.orientation
                            .get(last_entity)
                            .ok_or(repl::Error::MissingComponent(last_id, "Orientation"))?
                            .0;
                        let last_rot = Rotation2::new(last_angle).matrix().clone();
                        let last_attach_pos =
                            last_rot * Point2::new(-SEGMENT_LENGTH / 2.0, 0.0) + last_pos.coords;
                        let cur_distance = norm(&(last_attach_pos - owner_pos));

                        let target_distance =
                            (1.0 - new_lunch_timer / LUNCH_TIME_SECS) * SEGMENT_LENGTH;
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

                        /*let angle_def = constraint::Def::Angle { angle: 0.0 };
                        let angle_constraint = Constraint {
                            def: angle_def,
                            stiffness: 0.5,
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
                        data.constraints.add(angle_constraint);*/
                    }
                }
            }
            None => {
                if input.shoot {
                    // Start shooting the hook
                    hook_state.0 = Some(ActiveState {
                        num_active_segments: 0,
                        want_fix: true,
                        mode: Mode::Shooting,
                    });
                }
            }
        }

        // Maintain the `Active` flag of our segments
        let num_active_segments = match &hook_state.0 {
            &Some(ActiveState {
                num_active_segments,
                ..
            }) => num_active_segments as usize,
            &None => 0,
        };
        for i in 0..num_active_segments {
            data.active.insert(segment_entities[i], Active);
        }
        for i in num_active_segments..NUM_SEGMENTS {
            data.active.remove(segment_entities[i]);
        }

        // Join successive hook segments
        if num_active_segments > 1 {
            for i in 0..num_active_segments - 1 {
                let entity_a = segment_entities[i];
                let entity_b = segment_entities[i + 1];

                let joint_def = constraint::Def::Joint {
                    distance: 0.0,
                    p_object_a: Point2::new(-SEGMENT_LENGTH / 2.0, 0.0),
                    p_object_b: Point2::new(SEGMENT_LENGTH / 2.0, 0.0),
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
                //let stiffness = (j as f32 / NUM_SEGMENTS as f32).powi(2);
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

    Ok(())
}

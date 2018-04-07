use std::f32;

use nalgebra::{norm, zero, Point2, Vector2};

use specs::prelude::*;
use specs::storage::BTreeStorage;

use defs::{EntityId, GameInfo, INVALID_ENTITY_ID};
use entity::Active;
use event::{self, Event};
use game::ComponentType;
use physics::collision::{self, CollisionGroups, Cuboid, GeometricQueryType, ShapeHandle};
use physics::constraint::{self, Constraint};
use physics::sim::Constraints;
use physics::{self, interaction};
use physics::{AngularVelocity, Dynamic, Friction, InvAngularMass, InvMass, Orientation, Position,
              Update, Velocity};
use registry::Registry;
use repl;

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
            // TODO: Only send to owner
            ComponentType::Velocity,
            ComponentType::AngularVelocity,
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
            // TODO: Only send to owner
            ComponentType::Velocity,
            ComponentType::AngularVelocity,
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
        Some(first_segment_interaction),
    );
    interaction::set(
        reg,
        "first_hook_segment",
        "player",
        None,
        Some(first_segment_interaction),
    );
}

pub const NUM_SEGMENTS: usize = 20;
pub const SEGMENT_LENGTH: f32 = 50.0;
const JOIN_MARGIN: f32 = 5.0;
//const MAX_SHOOT_TIME_SECS: f32 = 2.0;
const SHOOT_SPEED: f32 = 1000.0;
const LUNCH_TIME_SECS: f32 = 0.025;
const LUNCH_RADIUS: f32 = 5.0;
const ANGLE_STIFFNESS: f32 = 0.7;
const OWNER_ANGLE_STIFFNESS: f32 = 0.0;

/// This event is emitted when a hook attaches at some point. This is meant to be used for
/// visualization purposes.
#[derive(Debug, Clone, BitStore)]
pub struct FixedEvent {
    /// Different hook colors for drawing.
    pub hook_index: u32,

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

impl repl::Predictable for Def {}

/// Definition of a hook segment. Again, this should not change in the entity's lifetime.
#[derive(Component, PartialEq, Clone, Debug, BitStore)]
pub struct SegmentDef {
    /// Every hook segment belongs to one hook.
    pub hook: EntityId,
}

impl repl::Predictable for SegmentDef {}

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

impl repl::Predictable for State {
    fn distance(&self, other: &State) -> f32 {
        // TODO: ActiveState distance
        if self != other {
            f32::INFINITY
        } else {
            0.0
        }
    }
}

/// Input for simulating a hook.
#[derive(Component, Clone, Debug)]
#[storage(BTreeStorage)]
pub struct CurrentInput {
    pub shoot: bool,
}

fn build_segment(builder: EntityBuilder) -> EntityBuilder {
    // TODO
    let shape = Cuboid::new(Vector2::new(SEGMENT_LENGTH / 2.0, 3.0));

    let mut groups = CollisionGroups::new();
    groups.set_membership(&[collision::GROUP_PLAYER_ENTITY]);
    groups.set_whitelist(&[collision::GROUP_WALL, collision::GROUP_PLAYER]);

    let query_type = GeometricQueryType::Contacts(0.0, 0.0);

    // TODO: Velocity (and Dynamic?) component should be added only for owners
    builder
        .with(Position(Point2::origin()))
        .with(Orientation(0.0))
        .with(Velocity(zero()))
        .with(AngularVelocity(0.0))
        .with(InvMass(1.0 / 5.0))
        .with(InvAngularMass(
            12.0 / (5.0 * (SEGMENT_LENGTH.powi(2) + 18.0)),
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
        assert!(repl::is_entity(world, owner));

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

#[derive(SystemData)]
struct InteractionData<'a> {
    entity_map: Fetch<'a, repl::EntityMap>,
    events: FetchMut<'a, event::Sink>,

    repl_id: ReadStorage<'a, repl::Id>,
    update: ReadStorage<'a, Update>,
    segment_def: ReadStorage<'a, SegmentDef>,
    hook_def: ReadStorage<'a, Def>,

    hook_state: WriteStorage<'a, State>,
}

fn first_segment_interaction(
    world: &World,
    segment_info: &interaction::EntityInfo,
    other_info: &interaction::EntityInfo,
    pos: Point2<f32>,
    normal: Vector2<f32>,
) -> Result<(), repl::Error> {
    let mut data = InteractionData::fetch(&world.res);

    // Can unwrap since every attachable entity has a `repl::Id`.
    let other_id = data.repl_id.get(other_info.entity).unwrap().0;

    if data.update.get(segment_info.entity).is_none() {
        // Don't attach segments that are not currently being simulated
        // FIXME: We need this because our collision filter is not narrow enough yet. For example,
        //        ncollide will report segment-wall interactions even of segments that are not
        //        currently being updated.
        return Ok(());
    }

    // Get the hook entity to which this segment belongs
    let hook_id = repl::try_component(&data.segment_def, segment_info.entity)?.hook;
    let hook_entity = data.entity_map.try_id_to_entity(hook_id)?;

    if other_id.0 == hook_id.0 {
        // Don't attach to entities that we own
        return Ok(());
    }

    let state = repl::try_component_mut(&mut data.hook_state, hook_entity)?;

    let active_state = state
        .0
        .as_mut()
        .expect("got segment wall interaction, but hook is inactive");

    if !active_state.want_fix {
        return Ok(());
    }

    // Only attach if we are not attached yet
    let (fixed, new_mode) = match &active_state.mode {
        &Mode::Shooting { .. } => (
            true,
            Mode::Contracting {
                lunch_timer: 0.0,
                fixed: Some((other_id, other_info.object_pos.coords.into())),
            },
        ),
        &Mode::Contracting {
            lunch_timer,
            fixed: None,
        } => (
            true,
            Mode::Contracting {
                lunch_timer,
                fixed: Some((other_id, other_info.object_pos.coords.into())),
            },
        ),
        mode => (false, mode.clone()),
    };

    active_state.mode = new_mode;

    if fixed && active_state.num_active_segments > 3 {
        let hook_def = repl::try_component(&data.hook_def, hook_entity)?;

        data.events.push(FixedEvent {
            hook_index: hook_def.index,
            pos: pos.coords.into(),
            vel: normal.into(),
        });
    }

    Ok(())
}

#[derive(SystemData)]
struct InputData<'a> {
    entities: Entities<'a>,

    game_info: Fetch<'a, GameInfo>,
    entity_map: Fetch<'a, repl::EntityMap>,

    constraints: FetchMut<'a, Constraints>,

    input: ReadStorage<'a, CurrentInput>,
    hook_def: ReadStorage<'a, Def>,

    active: WriteStorage<'a, Active>,
    position: WriteStorage<'a, Position>,
    velocity: WriteStorage<'a, Velocity>,
    orientation: WriteStorage<'a, Orientation>,
    angular_velocity: WriteStorage<'a, AngularVelocity>,
    hook_state: WriteStorage<'a, State>,
}

pub fn run_input_sys(world: &World) -> Result<(), repl::Error> {
    let mut data = InputData::fetch(&world.res);

    let dt = data.game_info.tick_duration_secs();

    // Update all hooks that currently have some input attached to them
    for (input, hook_def, hook_state) in (&data.input, &data.hook_def, &mut data.hook_state).join()
    {
        //debug!("{:?}", hook_state);

        // Need to know the position of our owner entity
        let owner_entity = data.entity_map.try_id_to_entity(hook_def.owner)?;
        let (owner_pos, owner_orientation, owner_velocity) = {
            let c = repl::try_join_get(
                owner_entity,
                &data.entities,
                (&data.position, &data.orientation, &data.velocity).join(),
            )?;

            (c.0.clone(), c.1.clone(), c.2.clone())
        };

        // Look up the segment entities of this hook
        let mut segment_entities = [owner_entity; NUM_SEGMENTS];
        for i in 0..NUM_SEGMENTS {
            segment_entities[i] = data.entity_map.try_id_to_entity(hook_def.segments[i])?;
        }

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
                        let last_pos = repl::try_component(&data.position, last_entity)?.0;
                        let distance = norm(&(last_pos - owner_pos.0));
                        distance >= SEGMENT_LENGTH / 2.0
                    };

                    let join_index = if activate_next {
                        if num_active_segments + 1 < NUM_SEGMENTS {
                            let segment_index = num_active_segments;
                            let next_segment = segment_entities[segment_index];

                            let vel = Vector2::new(
                                owner_orientation.0.cos(),
                                owner_orientation.0.sin(),
                            ) * SHOOT_SPEED;

                            //debug!("shoot at {}", owner_orientation.0);
                            data.position.insert(next_segment, Position(owner_pos.0));
                            data.orientation
                                .insert(next_segment, Orientation(owner_orientation.0));
                            data.velocity
                                .insert(next_segment, Velocity(owner_velocity.0 + vel));
                            data.angular_velocity
                                .insert(next_segment, AngularVelocity(0.0));

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
                    let last_entity = segment_entities[join_index];
                    let last_pos = repl::try_component(&data.position, last_entity)?.0;
                    let last_angle = repl::try_component(&data.orientation, last_entity)?.0;
                    let last_attach_pos = physics::to_world_pos(
                        last_pos,
                        last_angle,
                        Point2::new(-SEGMENT_LENGTH / 2.0 + JOIN_MARGIN, 0.0),
                    );
                    let cur_distance = norm(&(last_attach_pos - owner_pos.0));
                    let target_distance = 0.0;

                    if cur_distance > SEGMENT_LENGTH / 2.0 - JOIN_MARGIN {
                        let constraint_distance = cur_distance.min(target_distance);
                        data.constraints.add(owner_segment_joint_constraint(
                            owner_entity,
                            last_entity,
                            false,
                            constraint_distance,
                        ));
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
                            data.constraints.add(fix_first_segment_constraint(
                                segment_entities[0],
                                fix_entity,
                                fix_pos_object,
                            ));
                        } else {
                            warn!("hook attached to dead entity {:?}", fix_entity_id);
                        }
                    }

                    // Eat up the last segment if it comes close enough to our mouth.
                    let last_index = num_active_segments as usize - 1;
                    let last_entity = segment_entities[last_index];
                    let last_pos = repl::try_component(&data.position, last_entity)?.0;
                    let last_angle = repl::try_component(&data.orientation, last_entity)?.0;
                    let last_attach_pos = physics::to_world_pos(
                        last_pos,
                        last_angle,
                        Point2::new(-SEGMENT_LENGTH / 2.0 + JOIN_MARGIN, 0.0),
                    );
                    let cur_distance = norm(&(last_attach_pos - owner_pos.0));

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
                        let last_entity = segment_entities[last_index];
                        let last_pos = repl::try_component(&data.position, last_entity)?.0;
                        let last_angle = repl::try_component(&data.orientation, last_entity)?.0;
                        let last_attach_pos = physics::to_world_pos(
                            last_pos,
                            last_angle,
                            Point2::new(-SEGMENT_LENGTH / 2.0 + JOIN_MARGIN, 0.0),
                        );
                        let cur_distance = norm(&(last_attach_pos - owner_pos.0));

                        let target_distance =
                            (1.0 - new_lunch_timer / LUNCH_TIME_SECS) * SEGMENT_LENGTH;
                        let constraint_distance = cur_distance.min(target_distance);

                        data.constraints.add(owner_segment_joint_constraint(
                            owner_entity,
                            last_entity,
                            new_fixed.is_some(),
                            constraint_distance,
                        ));

                        if new_fixed.is_none() {
                            data.constraints
                                .add(owner_segment_angle_constraint(owner_entity, last_entity));
                        }
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

                data.constraints
                    .add(segments_joint_constraint(entity_a, entity_b));
                data.constraints
                    .add(segments_angle_constraint(entity_a, entity_b));
            }
        }
    }

    Ok(())
}

fn segments_joint_constraint(entity_a: Entity, entity_b: Entity) -> Constraint {
    let joint_def = constraint::Def::Joint {
        distance: 0.0,
        object_pos_a: Point2::new(-SEGMENT_LENGTH / 2.0 + JOIN_MARGIN, 0.0),
        object_pos_b: Point2::new(SEGMENT_LENGTH / 2.0 - JOIN_MARGIN, 0.0),
    };
    Constraint {
        def: joint_def,
        stiffness: 1.0,
        entity_a,
        entity_b,
        vars_a: constraint::Vars {
            pos: true,
            angle: true,
        },
        vars_b: constraint::Vars {
            pos: true,
            angle: true,
        },
    }
}

fn segments_angle_constraint(entity_a: Entity, entity_b: Entity) -> Constraint {
    let angle_def = constraint::Def::Angle { angle: 0.0 };
    let stiffness = ANGLE_STIFFNESS;
    Constraint {
        def: angle_def,
        stiffness: stiffness,
        entity_a,
        entity_b,
        vars_a: constraint::Vars {
            pos: false,
            angle: true,
        },
        vars_b: constraint::Vars {
            pos: false,
            angle: true,
        },
    }
}

fn owner_segment_joint_constraint(
    owner_entity: Entity,
    last_entity: Entity,
    owner_pos_var: bool,
    distance: f32,
) -> Constraint {
    let joint_def = constraint::Def::Joint {
        distance: distance.sqrt(),
        object_pos_a: Point2::new(1.0, 0.0), //Point2::origin(),
        object_pos_b: Point2::new(-SEGMENT_LENGTH / 2.0 + JOIN_MARGIN, 0.0),
    };
    Constraint {
        def: joint_def,
        stiffness: 1.0,
        entity_a: owner_entity,
        entity_b: last_entity,
        vars_a: constraint::Vars {
            pos: owner_pos_var,
            angle: false,
        },
        vars_b: constraint::Vars {
            pos: true,
            angle: true,
        },
    }
}

fn owner_segment_angle_constraint(owner_entity: Entity, last_entity: Entity) -> Constraint {
    let angle_def = constraint::Def::Angle { angle: 0.0 };
    Constraint {
        def: angle_def,
        stiffness: OWNER_ANGLE_STIFFNESS,
        entity_a: owner_entity,
        entity_b: last_entity,
        vars_a: constraint::Vars {
            pos: false,
            angle: false,
        },
        vars_b: constraint::Vars {
            pos: false,
            angle: true,
        },
    }
}

fn fix_first_segment_constraint(
    first_entity: Entity,
    fix_entity: Entity,
    fix_pos_object: [f32; 2],
) -> Constraint {
    let joint_def = constraint::Def::Joint {
        distance: 0.0,
        object_pos_a: Point2::new(SEGMENT_LENGTH / 2.0 - JOIN_MARGIN, 0.0),
        object_pos_b: Point2::new(fix_pos_object[0], fix_pos_object[1]),
    };
    Constraint {
        def: joint_def,
        stiffness: 1.0,
        entity_a: first_entity,
        entity_b: fix_entity,
        vars_a: constraint::Vars {
            pos: true,
            angle: true,
        },
        vars_b: constraint::Vars {
            pos: false,
            angle: false,
        },
    }
}

use nalgebra::zero;
use registry::Registry;
use defs::EntityId;
use repl;
use physics::interaction;
use physics::collision;
use physics::{AngularVelocity, Dynamic, Friction, InvAngularMass, InvMass, Orientation, Position,
              Velocity};
use game::ComponentType;

pub fn register(reg: &mut Registry) {
    reg.component::<Def>();
    reg.component::<SegmentDef>();
    reg.component::<State>();
    reg.component::<Input>();
    reg.component::<ActivateSegment>();
    reg.component::<DeactivateSegment>();

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
    /// Every hook belongs to one entity.
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
    Shooting { time_secs: f32 },
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
pub struct Input {
    pub shoot: bool,
}

#[derive(Component, Clone, Debug)]
#[component(BTreeStorage)]
struct ActivateSegment {
    position: Point2<f32>,
    velocity: Vector2<f32>,
    orientation: f32,
}

#[derive(Component, Clone, Debug, Default)]
#[component(NullStorage)]
struct DeactivateSegment;

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

    pub fn create(world: &mut World, owner: EntityId) {
        assert!(
            world
                .read_resource::<repl::EntityMap>()
                .get_id_to_entity(owner)
                .is_some(),
            "hook owner entity does not exist"
        );

        // Create hook
        let (hook_id, hook_entity) =
            repl::entity::auth::create(world, owner.0, "hook", |builder| builder.with(State(None)));

        // Create hook segments
        let mut segments = [INVALID_ENTITY_ID; HOOK_NUM_SEGMENTS];
        for i in 0..HOOK_NUM_SEGMENTS {
            let (segment_id, _) =
                repl::entity::auth::create(world, owner.0, "hook_segment", |builder| {
                    let def = SegmentDef { hook: hook_id };
                    builder.with(def);
                });
        }

        // Now that we have the IDs of all the segments, attach the definition to the hook
        world
            .write::<Def>()
            .insert(hook_entity, Def { owner, segments });
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
    // TODO: To make this unwrap okay, validate that received entities have the components
    //       specified by their class.
    let segment_def = world.read::<SegmentDef>().get(segment_entity).unwrap();

    // TODO: repl unwrap -- server could send faulty hook id in segment definition
    let hook_entity = repl::get_id_to_entity(world, segment_def.hook).unwrap();

    // TODO: To make this unwrap okay, validate that received entities have the components
    //       specified by their class.
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
    entity_map: Fetch<'a, EntityMap>,
    entities: Entities<'a>,
    constraints: FetchMut<'a, Constraints>,

    input: ReadStorage<'a, CurrentInput>,
    repl_id: ReadStorage<'a, repl::Id>,

    active: WriteStorage<'a, Active>,

    position: WriteStorage<'a, Position>,
    velocity: WriteStorage<'a, Velocity>,
    orientation: WriteStorage<'a, Orientation>,

    hook: WriteStorage<'a, Hook>,
    segment: WriteStorage<'a, HookSegment>,
    activate_segment: WriteStorage<'a, ActivateHookSegment>,
    deactivate_segment: WriteStorage<'a, DeactivateHookSegment>,
}

struct InputSys;

impl<'a> System<'a> for InputSys {}

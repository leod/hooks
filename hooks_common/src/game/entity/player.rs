use std::f32;

use nalgebra::{norm, zero, Point2, Rotation2, Vector2};

use specs::prelude::*;
use specs::storage::BTreeStorage;

use defs::{EntityId, GameInfo, PlayerId, PlayerInput, INVALID_ENTITY_ID};
use event::{self, Event};
use game::entity::hook;
use game::ComponentType;
use physics::collision::{self, CollisionGroups, Cuboid, GeometricQueryType, ShapeHandle};
use physics::interaction;
use physics::{AngularVelocity, Drag, Dynamic, InvAngularMass, InvMass, Orientation, Position,
              Velocity};
use registry::Registry;
use repl;

pub fn register(reg: &mut Registry) {
    reg.component::<InputState>();
    reg.component::<CurrentInput>();
    reg.component::<Player>();
    reg.component::<State>();

    reg.event::<DashedEvent>();

    repl::entity::register_class(
        reg,
        "player",
        &[
            ComponentType::Position,
            ComponentType::Orientation,
            ComponentType::Player,
            // TODO: Only send to owner
            ComponentType::Velocity,
            ComponentType::AngularVelocity,
            ComponentType::PlayerInputState,
            ComponentType::PlayerState,
        ],
        build_player,
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
        "player",
        "test",
        Some(interaction::Action::PreventOverlap {
            rotate_a: false,
            rotate_b: false,
        }),
        None,
    );

    // FIXME: Due to a bug in physics sim, other player also gets moved
    interaction::set(
        reg,
        "player",
        "player",
        Some(interaction::Action::PreventOverlap {
            rotate_a: false,
            rotate_b: false,
        }),
        None,
    );
}

pub const NUM_HOOKS: usize = 2;
pub const WIDTH: f32 = 40.0;
pub const HEIGHT: f32 = 40.0;
pub const MOVE_ACCEL: f32 = 3000.0;
pub const ROT_ACCEL: f32 = 200.0;
pub const MASS: f32 = 50.0;
pub const DRAG: f32 = 4.0;
pub const SNAP_ANGLE: f32 = f32::consts::PI / 12.0;
pub const MAX_ANGULAR_VEL: f32 = f32::consts::PI * 5.0;
pub const TAP_SECS: f32 = 0.25;
pub const DASH_SECS: f32 = 0.3;
pub const DASH_COOLDOWN_SECS: f32 = 2.0;
pub const DASH_ACCEL: f32 = 10000.0;

#[derive(Debug, Clone, BitStore)]
pub struct DashedEvent {
    /// Different hook colors for drawing.
    pub hook_index: u32,
}

impl Event for DashedEvent {
    fn class(&self) -> event::Class {
        event::Class::Order
    }
}


/// Component that is attached whenever player input should be executed for an entity.
#[derive(Component, Clone, Debug)]
#[storage(BTreeStorage)]
pub struct CurrentInput(pub PlayerInput);

// Tappable keys
const MOVE_FORWARD_KEY: usize = 0;
const MOVE_BACKWARD_KEY: usize = 1;
const MOVE_LEFT_KEY: usize = 2;
const MOVE_RIGHT_KEY: usize = 3;
const NUM_TAP_KEYS: usize = 4;

#[derive(PartialEq, Clone, Copy, Debug, Default, BitStore)]
struct TapState {
    secs_left: f32,
}

#[derive(Component, PartialEq, Clone, Copy, Debug, Default, BitStore)]
#[storage(BTreeStorage)]
pub struct InputState {
    previous_shoot_one: bool,
    previous_shoot_two: bool,
    previous_tap_input: [bool; NUM_TAP_KEYS],
    tap_state: [TapState; NUM_TAP_KEYS],
}

impl repl::Component for InputState {}

#[derive(Component, PartialEq, Clone, Copy, Debug, BitStore)]
#[storage(BTreeStorage)]
pub struct Player {
    pub hooks: [EntityId; NUM_HOOKS],
}

impl repl::Component for Player {
    const STATIC: bool = true;
}

#[derive(PartialEq, Clone, Copy, Debug, BitStore)]
pub struct DashState {
    pub direction: [f32; 2],
    pub secs_left: f32,
}

#[derive(Component, PartialEq, Clone, Copy, Debug, Default, BitStore)]
#[storage(BTreeStorage)]
pub struct State {
    pub dash_cooldown_secs: f32,
    pub dash_state: Option<DashState>,
}

impl repl::Component for State {}

impl State {
    pub fn dash(&mut self, direction: Vector2<f32>) {
        if self.dash_cooldown_secs == 0.0 {
            self.dash_cooldown_secs = DASH_COOLDOWN_SECS;
            self.dash_state = Some(DashState {
                direction: [direction.x, direction.y],
                secs_left: DASH_SECS,
            });
        }
    }

    pub fn update_dash(&mut self, dt: f32) {
        self.dash_cooldown_secs -= dt;
        if self.dash_cooldown_secs < 0.0 {
            self.dash_cooldown_secs = 0.0;
        }

        self.dash_state = self.dash_state.as_ref().and_then(|dash_state| {
            let secs_left = dash_state.secs_left - dt;

            if secs_left <= 0.0 {
                None
            } else {
                Some(DashState {
                    secs_left,
                    ..*dash_state
                })
            }
        });
    }
}

pub fn run_input(
    world: &mut World,
    entity: Entity,
    input: &PlayerInput,
) -> Result<(), repl::Error> {
    // Update hooks
    {
        let player = *repl::try(&world.read::<Player>(), entity)?;
        let input_state = *repl::try(&world.read::<InputState>(), entity)?;

        for i in 0..NUM_HOOKS {
            let hook_entity = repl::try_id_to_entity(world, player.hooks[i])?;
            let hook_input = hook::CurrentInput {
                rot_angle: input.rot_angle,
                shoot: if i == 0 {
                    input.shoot_one
                } else {
                    input.shoot_two
                },
                previous_shoot: if i == 0 {
                    input_state.previous_shoot_one
                } else {
                    input_state.previous_shoot_two
                },
                pull: if i == 0 {
                    input.pull_one
                } else {
                    input.pull_two
                },
            };

            world
                .write::<hook::CurrentInput>()
                .insert(hook_entity, hook_input);
        }

        hook::run_input(&world)?;
    }

    // Update player
    {
        world
            .write::<CurrentInput>()
            .insert(entity, CurrentInput(input.clone()));

        InputSys.run_now(&world.res);
    }

    Ok(())
}

pub fn run_input_post_sim(
    world: &mut World,
    _entity: Entity,
    _input: &PlayerInput,
) -> Result<(), repl::Error> {
    hook::run_input_post_sim(&world)?;

    world.write::<hook::CurrentInput>().clear();
    world.write::<CurrentInput>().clear();

    Ok(())
}

pub mod auth {
    use super::*;

    pub fn create(world: &mut World, owner: PlayerId, pos: Point2<f32>) -> (EntityId, Entity) {
        let (id, entity) = repl::entity::auth::create(world, owner, "player", |builder| {
            builder.with(Position(pos))
        });

        let mut hooks = [INVALID_ENTITY_ID; NUM_HOOKS];
        for (i, hook) in hooks.iter_mut().enumerate() {
            let (hook_id, _) = hook::auth::create(world, id, i as u32);
            *hook = hook_id;
        }

        // Now that we have created our hooks, attach the player definition
        world.write::<Player>().insert(entity, Player { hooks });

        (id, entity)
    }
}

fn build_player(builder: EntityBuilder) -> EntityBuilder {
    let shape = Cuboid::new(Vector2::new(WIDTH / 2.0, HEIGHT / 2.0));

    let mut groups = CollisionGroups::new();
    groups.set_membership(&[collision::GROUP_PLAYER]);
    groups.set_whitelist(&[
        collision::GROUP_PLAYER,
        collision::GROUP_WALL,
        collision::GROUP_PLAYER_ENTITY,
        collision::GROUP_NEUTRAL,
    ]);

    let query_type = GeometricQueryType::Contacts(0.0, 0.0);

    // TODO: Velocity (and Dynamic?) component should be added only for owners
    builder
        .with(Orientation(0.0))
        .with(Velocity(zero()))
        .with(AngularVelocity(0.0))
        .with(InvMass(1.0 / MASS))
        .with(InvAngularMass(1.0 / 10.0))
        .with(Dynamic)
        .with(Drag(DRAG))
        .with(collision::Shape(ShapeHandle::new(shape)))
        .with(collision::Object { groups, query_type })
        .with(InputState::default())
        .with(State::default())
}

#[derive(SystemData)]
struct InputData<'a> {
    game_info: Fetch<'a, GameInfo>,
    input: ReadStorage<'a, CurrentInput>,

    orientation: WriteStorage<'a, Orientation>,
    velocity: WriteStorage<'a, Velocity>,
    angular_velocity: WriteStorage<'a, AngularVelocity>,
    state: WriteStorage<'a, State>,
    input_state: WriteStorage<'a, InputState>,
}

struct InputSys;

impl<'a> System<'a> for InputSys {
    type SystemData = InputData<'a>;

    fn run(&mut self, mut data: InputData<'a>) {
        let dt = data.game_info.tick_duration_secs();

        // Update tap state
        let mut tapped_keys = [false; NUM_TAP_KEYS];
        for (input, input_state) in (&data.input, &mut data.input_state).join() {
            let tap_input = [
                input.0.move_forward,
                input.0.move_backward,
                input.0.move_left,
                input.0.move_right,
            ];

            for i in 0..NUM_TAP_KEYS {
                if tap_input[i] && !input_state.previous_tap_input[i] {
                    if input_state.tap_state[i].secs_left > 0.0 {
                        tapped_keys[i] = true;
                        input_state.tap_state[i].secs_left = 0.0;
                    } else {
                        input_state.tap_state[i].secs_left = TAP_SECS;
                    }
                }

                input_state.tap_state[i].secs_left -= dt;
                if input_state.tap_state[i].secs_left < 0.0 {
                    input_state.tap_state[i].secs_left = 0.0;
                }

                input_state.previous_tap_input[i] = tap_input[i];
            }
        }

        // Movement
        for (input, orientation, velocity, angular_velocity, state) in (
            &data.input,
            &mut data.orientation,
            &mut data.velocity,
            &mut data.angular_velocity,
            &mut data.state,
        ).join()
        {
            // Dashing
            let forward = Rotation2::new(orientation.0).matrix() * Vector2::new(1.0, 0.0);
            let right = Vector2::new(-forward.y, forward.x);

            if tapped_keys[MOVE_FORWARD_KEY] {
                state.dash(forward);
            }
            if tapped_keys[MOVE_BACKWARD_KEY] {
                state.dash(-forward);
            }
            if tapped_keys[MOVE_RIGHT_KEY] {
                state.dash(right);
            }
            if tapped_keys[MOVE_LEFT_KEY] {
                state.dash(-right);
            }

            state.update_dash(dt);

            if let Some(dash_state) = state.dash_state.as_ref() {
                velocity.0 += Vector2::new(dash_state.direction[0], dash_state.direction[1]) *
                    DASH_ACCEL * dt;
                continue;
            }

            /*if input.0.rot_angle != orientation.0 {
                // TODO: Only mutate if changed
                orientation.0 = input.0.rot_angle;
            }*/

            let diff = (input.0.rot_angle - orientation.0 + f32::consts::PI) %
                (2.0 * f32::consts::PI) - f32::consts::PI;
            let smallest_angle = if diff < -f32::consts::PI {
                diff + 2.0 * f32::consts::PI
            } else {
                diff
            };
            if smallest_angle.abs() <= SNAP_ANGLE {
                orientation.0 = input.0.rot_angle;
            } else if smallest_angle < 0.0 {
                angular_velocity.0 -= ROT_ACCEL * dt;
            } else if smallest_angle > 0.0 {
                angular_velocity.0 += ROT_ACCEL * dt;
            }

            if angular_velocity.0.abs() > MAX_ANGULAR_VEL {
                angular_velocity.0 = angular_velocity.0.signum() * MAX_ANGULAR_VEL;
            }

            let forward = Rotation2::new(orientation.0).matrix() * Vector2::new(1.0, 0.0);
            let right = Vector2::new(-forward.y, forward.x);

            let mut direction = Vector2::new(0.0, 0.0);

            if input.0.move_forward {
                direction += forward;
            }
            if input.0.move_backward {
                direction -= forward;
            }
            if input.0.move_right {
                direction += right;
            }
            if input.0.move_left {
                direction -= right;
            }

            let direction_norm = norm(&direction);
            if direction_norm > 0.0 {
                velocity.0 += direction / direction_norm * MOVE_ACCEL * dt;
                //velocity.0 += direction / direction_norm * 25.0;
            }
        }

        // Remember some input state
        for (input, input_state) in (&data.input, &mut data.input_state).join() {
            input_state.previous_shoot_one = input.0.shoot_one;
            input_state.previous_shoot_two = input.0.shoot_two;
        }
    }
}

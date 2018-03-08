use nalgebra::{normalize, zero, Point2, Rotation2, Vector2};
use specs::{BTreeStorage, Entity, EntityBuilder, Fetch, Join, ReadStorage, RunNow, System, World,
            WriteStorage};

use defs::{EntityId, GameInfo, PlayerId, PlayerInput, INVALID_ENTITY_ID};
use registry::Registry;
use physics::interaction;
use physics::{AngularVelocity, Drag, Dynamic, InvAngularMass, InvMass, Orientation, Position,
              Velocity};
use physics::collision::{self, CollisionGroups, Cuboid, GeometricQueryType, ShapeHandle};
use repl;
use game::ComponentType;
use game::entity::hook;

pub fn register(reg: &mut Registry) {
    reg.component::<CurrentInput>();
    reg.component::<Player>();

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
}

pub const NUM_HOOKS: usize = 2;
const MOVE_ACCEL: f32 = 2000.0;

/// Component that is attached whenever player input should be executed for an entity.
#[derive(Component, Clone, Debug)]
#[component(BTreeStorage)]
pub struct CurrentInput(pub PlayerInput);

#[derive(Component, PartialEq, Clone, Debug, BitStore)]
#[component(BTreeStorage)]
pub struct Player {
    pub hooks: [EntityId; NUM_HOOKS],
}

pub fn run_input(
    world: &mut World,
    entity: Entity,
    input: &PlayerInput,
) -> Result<(), repl::Error> {
    // Update player
    {
        world
            .write::<CurrentInput>()
            .insert(entity, CurrentInput(input.clone()));

        InputSys.run_now(&world.res);

        world.write::<CurrentInput>().clear();
    }

    // Update hooks
    {
        let player = world
            .read::<Player>()
            .get(entity)
            .ok_or_else(|| {
                repl::Error::Replication("player entity without Player component".to_string())
            })?
            .clone();

        for i in 0..NUM_HOOKS {
            let hook_entity = repl::try_id_to_entity(world, player.hooks[i])?;

            world.write::<hook::CurrentInput>().insert(
                hook_entity,
                hook::CurrentInput {
                    shoot: if i == 0 {
                        input.shoot_one
                    } else {
                        input.shoot_two
                    },
                },
            );
        }

        hook::run_input_sys(&world)?;

        world.write::<hook::CurrentInput>().clear();
    }

    Ok(())
}

pub mod auth {
    use super::*;

    pub fn create(world: &mut World, owner: PlayerId, pos: Point2<f32>) -> (EntityId, Entity) {
        let (id, entity) = repl::entity::auth::create(world, owner, "player", |builder| {
            builder.with(Position(pos))
        });

        let mut hooks = [INVALID_ENTITY_ID; NUM_HOOKS];
        for i in 0..NUM_HOOKS {
            let (hook_id, _) = hook::auth::create(world, id, i as u32);
            hooks[i] = hook_id;
        }

        // Now that we have created our hooks, attach the player definition
        world.write::<Player>().insert(entity, Player { hooks });

        (id, entity)
    }
}

fn build_player(builder: EntityBuilder) -> EntityBuilder {
    let shape = Cuboid::new(Vector2::new(20.0, 20.0));

    let mut groups = CollisionGroups::new();
    groups.set_membership(&[collision::GROUP_PLAYER]);
    groups.set_whitelist(&[collision::GROUP_WALL]);

    let query_type = GeometricQueryType::Contacts(0.0, 0.0);

    // TODO: Velocity (and Dynamic?) component should be added only for owners
    builder
        .with(Orientation(0.0))
        .with(Velocity(zero()))
        .with(AngularVelocity(0.0))
        .with(InvMass(1.0 / 200.0))
        .with(InvAngularMass(1.0 / 10.0))
        .with(Dynamic)
        .with(Drag(200.0 * 7.5))
        .with(collision::Shape(ShapeHandle::new(shape)))
        .with(collision::Object { groups, query_type })
}

#[derive(SystemData)]
struct InputData<'a> {
    game_info: Fetch<'a, GameInfo>,
    input: ReadStorage<'a, CurrentInput>,
    velocity: WriteStorage<'a, Velocity>,
    orientation: WriteStorage<'a, Orientation>,
}

struct InputSys;

impl<'a> System<'a> for InputSys {
    type SystemData = InputData<'a>;

    fn run(&mut self, mut data: InputData<'a>) {
        let dt = data.game_info.tick_duration_secs();

        // Movement
        for (input, orientation, velocity) in
            (&data.input, &mut data.orientation, &mut data.velocity).join()
        {
            if input.0.rot_angle != orientation.0 {
                // TODO: Only mutate if changed
                orientation.0 = input.0.rot_angle;
            }

            let forward = Rotation2::new(orientation.0).matrix() * Vector2::new(1.0, 0.0);
            let right = Vector2::new(-forward.y, forward.x);

            let mut direction = Vector2::new(0.0, 0.0);
            let move_any = input.0.move_forward || input.0.move_backward || input.0.move_right ||
                input.0.move_left;

            if move_any {
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

                velocity.0 += normalize(&direction) * MOVE_ACCEL * dt;
            }
        }
    }
}

use nalgebra::{norm, zero, Point2, Vector2};

use specs::{self, Entities, Entity, Fetch, FetchMut, Join, ReadStorage, RunNow, System,
            VecStorage, World, WriteStorage};

use hooks_util::profile;
use defs::GameInfo;
use entity::{self, Active};
use registry::Registry;

use physics::{collision, constraint, interaction, AngularVelocity, Dynamic, Friction,
              InvAngularMass, InvMass, Joints, Orientation, Position, Velocity};
use physics::collision::CollisionWorld;
use physics::constraint::Constraint;

pub fn register(reg: &mut Registry) {
    reg.component::<Force>();

    reg.resource(Interactions(Vec::new()));
    reg.resource(Constraints(Vec::new()));
}

const JOINT_MIN_DISTANCE: f32 = 0.001;
const MIN_SPEED: f32 = 0.01;

/// Resource to store the interactions that were detected in a time step.
struct Interactions(Vec<(Entity, Entity, Point2<f32>)>);

/// Resource to store the constraints that are to be applied in the current time step.
struct Constraints(Vec<Constraint>);

impl Constraints {
    pub fn add(&mut self, c: Constraint) {
        self.0.push(c);
    }
}

/// For now, it seems that putting the whole physics simulation into a set of systems would be
/// clumsy. For example, to resolve collisions with impulses, we might need to iterate some systems
/// multiple times. However, systems don't seem to be easily composable with specs.
///
/// Thus, we are putting the simulation into this function.
pub fn run(world: &World) {
    profile!("physics");

    collision::MaintainSys.run_now(&world.res);
    collision::UpdateSys.run_now(&world.res);

    PrepareSys.run_now(&world.res);
    FrictionForceSys.run_now(&world.res);
    JointForceSys.run_now(&world.res);
    IntegrateForceSys.run_now(&world.res);
    HandleContactsSys.run_now(&world.res);
    SolveConstraintsSys.run_now(&world.res);
    IntegrateVelocitySys.run_now(&world.res);

    let interactions = world.read_resource::<Interactions>().0.clone();
    for &(entity_a, entity_b, pos) in &interactions {
        interaction::run(world, entity_a, entity_b, pos);
    }

    world.write_resource::<Constraints>().0.clear();
}

#[derive(Component)]
#[component(VecStorage)]
struct OldPosition(Point2<f32>);

#[derive(Component)]
#[component(VecStorage)]
struct Force(Vector2<f32>);

struct PrepareSys;

impl<'a> System<'a> for PrepareSys {
    type SystemData = (
        FetchMut<'a, Interactions>,
        Entities<'a>,
        ReadStorage<'a, Dynamic>,
        WriteStorage<'a, Force>,
    );

    fn run(&mut self, (mut interactions, entities, dynamic, mut force): Self::SystemData) {
        for (entity, _) in (&*entities, &dynamic).join() {
            force.insert(entity, Force(zero()));
        }

        interactions.0.clear();
    }
}

struct FrictionForceSys;

impl<'a> System<'a> for FrictionForceSys {
    type SystemData = (
        ReadStorage<'a, Active>,
        ReadStorage<'a, Dynamic>,
        ReadStorage<'a, Friction>,
        WriteStorage<'a, Velocity>,
        WriteStorage<'a, Force>,
    );

    fn run(&mut self, (active, dynamic, friction, mut velocity, mut force): Self::SystemData) {
        for (active, _, friction, velocity, force) in
            (&active, &dynamic, &friction, &mut velocity, &mut force).join()
        {
            if !active.0 {
                continue;
            }

            let speed = norm(&velocity.0);

            if speed < MIN_SPEED {
                velocity.0 = zero();
            } else {
                force.0 -= velocity.0 * friction.0;
            }
        }
    }
}

struct JointForceSys;

impl<'a> System<'a> for JointForceSys {
    type SystemData = (
        ReadStorage<'a, Active>,
        ReadStorage<'a, Dynamic>,
        ReadStorage<'a, Joints>,
        ReadStorage<'a, Position>,
        WriteStorage<'a, Force>,
    );

    fn run(&mut self, (active, dynamic, joints, positions, mut force): Self::SystemData) {
        return;

        for (is_active, _, joints, position_a, force) in
            (&active, &dynamic, &joints, &positions, &mut force).join()
        {
            if !is_active.0 {
                continue;
            }

            for &(entity_b, ref joint) in &joints.0 {
                if !active.get(entity_b).unwrap().0 {
                    // Both endpoints of the joint need to be active
                    continue;
                }

                // TODO: Should we lazily remove joints whose endpoint entity no longer exists?
                //       => Probably better to do it in a `RemovalSys`. We don't need this
                //          currently as all joints are created in "immediate mode".

                let position_b = positions.get(entity_b).unwrap();

                let delta = position_b.0 - position_a.0;
                let distance = norm(&delta);
                let r = distance - joint.resting_length;

                let sym = true;

                if sym {
                    if distance < JOINT_MIN_DISTANCE && joint.resting_length > 0.0 {
                        // TODO: Joint force if distance is close to zero
                        force.0 += joint.stiffness * r * Vector2::new(1.0, 0.0);
                    } else if distance >= JOINT_MIN_DISTANCE {
                        //if t.abs() >= JOINT_MIN_DISTANCE {
                        force.0 += joint.stiffness * r * delta / distance;
                    }
                } else {
                    if r > JOINT_MIN_DISTANCE {
                        force.0 += joint.stiffness * r * delta / distance;
                    }
                }
            }
        }
    }
}

struct IntegrateForceSys;

impl<'a> System<'a> for IntegrateForceSys {
    type SystemData = (
        Fetch<'a, GameInfo>,
        ReadStorage<'a, Active>,
        ReadStorage<'a, InvMass>,
        ReadStorage<'a, Dynamic>,
        ReadStorage<'a, Force>,
        WriteStorage<'a, Velocity>,
    );

    fn run(
        &mut self,
        (game_info, active, inv_mass, dynamic, force, mut velocity): Self::SystemData,
    ) {
        let dt = game_info.tick_duration_secs() as f32;

        for (active, _, inv_mass, force, velocity) in
            (&active, &dynamic, &inv_mass, &force, &mut velocity).join()
        {
            if !active.0 {
                continue;
            }

            velocity.0 += force.0 * inv_mass.0 * dt;
        }
    }
}

struct HandleContactsSys;

impl<'a> System<'a> for HandleContactsSys {
    type SystemData = (
        Fetch<'a, GameInfo>,
        Fetch<'a, CollisionWorld>,
        Fetch<'a, interaction::Handlers>,
        FetchMut<'a, Interactions>,
        FetchMut<'a, Constraints>,
        ReadStorage<'a, entity::Meta>,
        ReadStorage<'a, Active>,
        ReadStorage<'a, Position>,
    );

    #[cfg_attr(rustfmt, rustfmt_skip)] // rustfmt bug
    fn run(
        &mut self,
        (
            game_info,
            collision_world,
            interaction_handlers,
            mut interactions,
            mut constraints,
            meta,
            active,
            position,
        ): Self::SystemData
    ) {
        let dt = game_info.tick_duration_secs() as f32;

        for (oa, ob, gen) in collision_world.contact_pairs() {
            let mut contacts = Vec::new();
            gen.contacts(&mut contacts);

            for contact in &contacts {
                let entity_a = *oa.data();
                let entity_b = *ob.data();

                /*if action == Some(interaction::Action::PreventOverlap) {
                    let constraint = Constraint {
                        entity_a: *oa.data(),
                        entity_b: *ob.data(),
                    };
                }

                // TODO: Fix this position
                let pos = contact.world1;

                interactions.0.push((entity_a, entity_b, pos));*/
            }
        }
    }
}

struct SolveConstraintsSys;

impl<'a> System<'a> for SolveConstraintsSys {
    type SystemData = (
        Fetch<'a, GameInfo>,
        Fetch<'a, Constraints>,
        ReadStorage<'a, InvMass>,
        ReadStorage<'a, InvAngularMass>,
        ReadStorage<'a, Position>,
        ReadStorage<'a, Orientation>,
        WriteStorage<'a, Velocity>,
        WriteStorage<'a, AngularVelocity>,
    );

    #[cfg_attr(rustfmt, rustfmt_skip)] // rustfmt bug
    fn run(
        &mut self,
        (
            game_info,
            constraints,
            inv_mass,
            inv_angular_mass,
            position,
            orientation,
            mut velocity,
            mut angular_velocity,
        ): Self::SystemData
    ) {
        let dt = game_info.tick_duration_secs() as f32;

        let num_iterations = 4;

        for _ in 1..num_iterations {
            for c in &constraints.0 {
                // Set up input for constraint solving
                let x_a = constraint::Position {
                    p: position.get(c.entity_a).unwrap().0,
                    angle: orientation.get(c.entity_a).unwrap().0,
                };
                let x_b = constraint::Position {
                    p: position.get(c.entity_b).unwrap().0,
                    angle: orientation.get(c.entity_b).unwrap().0,
                };
                let v_a = constraint::Velocity {
                    linear: velocity.get(c.entity_a).unwrap().0,
                    angular: angular_velocity.get(c.entity_a).unwrap().0,
                };
                let v_b = constraint::Velocity {
                    linear: velocity.get(c.entity_b).unwrap().0,
                    angular: angular_velocity.get(c.entity_b).unwrap().0,
                };
                let m_a = constraint::Mass {
                    inv: inv_mass.get(c.entity_a).unwrap().0,
                    inv_angular: inv_angular_mass.get(c.entity_a).unwrap().0,
                };
                let m_b = constraint::Mass {
                    inv: inv_mass.get(c.entity_b).unwrap().0,
                    inv_angular: inv_angular_mass.get(c.entity_b).unwrap().0,
                };
                let beta = 0.2;

                let (v_new_a, v_new_b) = constraint::solve_for_velocity(
                    &c.def,
                    &x_a,
                    &x_b,
                    &v_a,
                    &v_b,
                    &m_a,
                    &m_b,
                    beta,
                    dt
                );

                velocity.insert(c.entity_a, Velocity(v_new_a.linear));
                velocity.insert(c.entity_b, Velocity(v_new_b.linear));

                angular_velocity.insert(c.entity_a, AngularVelocity(v_new_a.angular));
                angular_velocity.insert(c.entity_b, AngularVelocity(v_new_b.angular));
            }
        }
    }
}

struct IntegrateVelocitySys;

impl<'a> System<'a> for IntegrateVelocitySys {
    type SystemData = (
        Fetch<'a, GameInfo>,
        ReadStorage<'a, Active>,
        ReadStorage<'a, Dynamic>,
        ReadStorage<'a, Velocity>,
        ReadStorage<'a, AngularVelocity>,
        WriteStorage<'a, Position>,
        WriteStorage<'a, Orientation>,
    );

    #[cfg_attr(rustfmt, rustfmt_skip)] // rustfmt bug
    fn run(
        &mut self,
        (
            game_info,
            active,
            dynamic,
            velocity,
            angular_velocity,
            mut position,
            mut orientation
        ): Self::SystemData
    ) {
        let dt = game_info.tick_duration_secs() as f32;

        for (active, _, velocity, angular_velocity, position, orientation) in (
            &active,
            &dynamic,
            &velocity,
            &angular_velocity,
            &mut position,
            &mut orientation,
        ).join()
        {
            if !active.0 {
                continue;
            }

            position.0 += velocity.0 * dt;
            orientation.0 += angular_velocity.0 * dt;
        }
    }
}

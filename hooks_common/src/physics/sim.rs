use std::f32;

use nalgebra::{norm, zero, Point2, Vector2};

use specs::{Entities, Entity, Fetch, FetchMut, Join, ReadStorage, RunNow, System, VecStorage,
            World, WriteStorage};

use hooks_util::profile;
use defs::GameInfo;
use entity::{self, Active};
use registry::Registry;

use physics::{collision, constraint, interaction, AngularVelocity, Dynamic, Friction,
              InvAngularMass, InvMass, Joints, Orientation, Position, Update, Velocity};
use physics::collision::CollisionWorld;
use physics::constraint::Constraint;

pub fn register(reg: &mut Registry) {
    reg.component::<OldPosition>();
    reg.component::<OldOrientation>();
    reg.component::<Force>();

    reg.resource(InteractionEvents(Vec::new()));
    reg.resource(Constraints(Vec::new()));
}

/// Tag components that all need to be given for entities that want to be simulated.
#[derive(SystemData)]
struct Filter<'a> {
    /// The entity is declared to be dynamic, as opposed to static entities like walls.
    dynamic: ReadStorage<'a, Dynamic>,

    /// The entity is currently present in the game.
    active: ReadStorage<'a, Active>,

    /// The entity is to be simulated in the next run call. This makes it possible to e.g. simulate
    /// only one player's entities.
    update: ReadStorage<'a, Update>,
}

impl<'a> Filter<'a> {
    pub fn join(
        &self,
    ) -> (
        &ReadStorage<'a, Dynamic>,
        &ReadStorage<'a, Active>,
        &ReadStorage<'a, Update>,
    ) {
        (&self.dynamic, &self.active, &self.update)
    }

    pub fn filter(&self, entity: Entity) -> bool {
        // TODO: Replace with new specs Join interface when updated
        self.dynamic.get(entity).is_some() && self.active.get(entity).is_some() &&
            self.update.get(entity).is_some()
    }
}

const JOINT_MIN_DISTANCE: f32 = 0.001;
const MIN_SPEED: f32 = 0.01;

#[derive(Component, PartialEq, Clone, Debug)]
#[component(VecStorage)]
struct OldPosition(Point2<f32>);

#[derive(Component, PartialEq, Clone, Debug)]
#[component(VecStorage)]
struct OldOrientation(f32);

/// Resource to store the interactions that were detected in a time step.
struct InteractionEvents(Vec<interaction::Event>);

/// Resource to store the constraints that are to be applied in the current time step.
pub struct Constraints(Vec<Constraint>);

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
    //JointForceSys.run_now(&world.res);
    IntegrateForceSys.run_now(&world.res);
    SavePositionSys.run_now(&world.res);
    IntegrateVelocitySys.run_now(&world.res);
    HandleContactsSys.run_now(&world.res);
    SolveConstraintsSys.run_now(&world.res);
    CorrectVelocitySys.run_now(&world.res);

    let interactions = world.read_resource::<InteractionEvents>().0.clone();
    for event in &interactions {
        interaction::run(world, event);
    }

    world.write_resource::<Constraints>().0.clear();
}

fn normalize_angle(angle: f32) -> f32 {
    angle
    //angle - 2.0 * f32::consts::PI * ((angle + f32::consts::PI) / (2.0 * f32::consts::PI)).floor()
}

#[derive(Component)]
#[component(VecStorage)]
struct Force(Vector2<f32>);

struct PrepareSys;

impl<'a> System<'a> for PrepareSys {
    type SystemData = (
        FetchMut<'a, InteractionEvents>,
        Entities<'a>,
        Filter<'a>,
        WriteStorage<'a, Force>,
    );

    fn run(&mut self, (mut interactions, entities, filter, mut force): Self::SystemData) {
        for (entity, _) in (&*entities, filter.join()).join() {
            force.insert(entity, Force(zero()));
        }

        interactions.0.clear();
    }
}

struct FrictionForceSys;

impl<'a> System<'a> for FrictionForceSys {
    type SystemData = (
        Filter<'a>,
        ReadStorage<'a, Friction>,
        WriteStorage<'a, Velocity>,
        WriteStorage<'a, Force>,
    );

    fn run(&mut self, (filter, friction, mut velocity, mut force): Self::SystemData) {
        for (_, friction, velocity, force) in
            (filter.join(), &friction, &mut velocity, &mut force).join()
        {
            let speed = norm(&velocity.0);

            if speed < MIN_SPEED {
                velocity.0 = zero();
            } else {
                force.0 -= velocity.0 / speed * friction.0;
                //force.0 -= velocity.0 * friction.0;
            }
        }
    }
}

struct JointForceSys;

impl<'a> System<'a> for JointForceSys {
    type SystemData = (
        Filter<'a>,
        ReadStorage<'a, Joints>,
        ReadStorage<'a, Position>,
        WriteStorage<'a, Force>,
    );

    fn run(&mut self, (filter, joints, positions, mut force): Self::SystemData) {
        for (_, joints, position_a, force) in
            (filter.join(), &joints, &positions, &mut force).join()
        {
            for &(entity_b, ref joint) in &joints.0 {
                if filter.active.get(entity_b).is_none() {
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

struct SavePositionSys;

impl<'a> System<'a> for SavePositionSys {
    type SystemData = (
        Entities<'a>,
        Filter<'a>,
        ReadStorage<'a, Position>,
        ReadStorage<'a, Orientation>,
        WriteStorage<'a, OldPosition>,
        WriteStorage<'a, OldOrientation>,
    );

    #[cfg_attr(rustfmt, rustfmt_skip)] // rustfmt bug
    fn run(
        &mut self,
        (
            entities,
            filter,
            position,
            orientation,
            mut old_position,
            mut old_orientation,
        ): Self::SystemData
    ) {
        for (entity, _, position) in (&*entities, filter.join(), &position).join() {
            old_position.insert(entity, OldPosition(position.0.clone()));
        }
        for (entity, _, orientation) in (&*entities, filter.join(), &orientation).join() {
            old_orientation.insert(entity, OldOrientation(orientation.0.clone()));
        }
    }
}

struct CorrectVelocitySys;

impl<'a> System<'a> for CorrectVelocitySys {
    type SystemData = (
        Fetch<'a, GameInfo>,
        Filter<'a>,
        ReadStorage<'a, Position>,
        ReadStorage<'a, OldPosition>,
        ReadStorage<'a, Orientation>,
        ReadStorage<'a, OldOrientation>,
        WriteStorage<'a, Velocity>,
        WriteStorage<'a, AngularVelocity>,
    );

    #[cfg_attr(rustfmt, rustfmt_skip)] // rustfmt bug
    fn run(
        &mut self,
        (
            game_info,
            filter,
            position,
            old_position,
            orientation,
            old_orientation,
            mut velocity,
            mut angular_velocity,
        ): Self::SystemData
    ) {
        let dt = game_info.tick_duration_secs();

        for (_, position, old_position, velocity) in
            (filter.join(), &position, &old_position, &mut velocity).join()
        {
            velocity.0 = (position.0 - old_position.0) / dt;
        }
        for (_, orientation, old_orientation, angular_velocity) in
            (filter.join(), &orientation, &old_orientation, &mut angular_velocity).join()
        {
            let x = orientation.0;
            let y = old_orientation.0;

            // TODO: trigonometric functions are not necessary to find minimal angle
            let d = (x - y).sin().atan2((x - y).cos());
            //let d = x - y;

            angular_velocity.0 = d / dt;
        }
    }
}

struct IntegrateForceSys;

impl<'a> System<'a> for IntegrateForceSys {
    type SystemData = (
        Fetch<'a, GameInfo>,
        Filter<'a>,
        ReadStorage<'a, InvMass>,
        ReadStorage<'a, Force>,
        WriteStorage<'a, Velocity>,
        WriteStorage<'a, AngularVelocity>,
    );

    #[cfg_attr(rustfmt, rustfmt_skip)] // rustfmt bug
    fn run(
        &mut self,
        (
            game_info,
            filter,
            inv_mass,
            force,
            mut velocity,
            mut ang_velocity,
        ): Self::SystemData,
    ) {
        let dt = game_info.tick_duration_secs();

        for (_, inv_mass, force, velocity, ang_velocity) in (
            filter.join(),
            &inv_mass,
            &force,
            &mut velocity,
            &mut ang_velocity,
        ).join()
        {
            velocity.0 += force.0 * inv_mass.0 * dt;

            // TODO: Angular friction
            if ang_velocity.0.abs() > 0.01 {
                let signum = ang_velocity.0.signum();
                ang_velocity.0 -= 100.0 * signum * dt;
                if ang_velocity.0.signum() != signum {
                    ang_velocity.0 = 0.0;
                }
            } else {
                ang_velocity.0 = 0.0;
            }
        }
    }
}

struct HandleContactsSys;

impl<'a> System<'a> for HandleContactsSys {
    type SystemData = (
        Fetch<'a, CollisionWorld>,
        Fetch<'a, interaction::Handlers>,
        FetchMut<'a, InteractionEvents>,
        FetchMut<'a, Constraints>,
        Filter<'a>,
        ReadStorage<'a, entity::Meta>,
        ReadStorage<'a, Velocity>,
    );

    #[cfg_attr(rustfmt, rustfmt_skip)] // rustfmt bug
    fn run(
        &mut self,
        (
            collision_world,
            interaction_handlers,
            mut interactions,
            mut constraints,
            filter,
            meta,
            velocity,
        ): Self::SystemData
    ) {
        for (oa, ob, gen) in collision_world.contact_pairs() {
            let mut contacts = Vec::new();
            gen.contacts(&mut contacts);

            for contact in &contacts {
                let entity_a = *oa.data();
                let entity_b = *ob.data();

                // Only consider contacts where at least one object is currently being simulated
                // FIXME: We should be able to get this with an ncollide filter!
                if !filter.filter(entity_a) && !filter.filter(entity_b) {
                    continue;
                }

                let action = interaction::get_action(
                    &interaction_handlers,
                    &meta,
                    entity_a,
                    entity_b
				);

                // TODO: Easier way to get object-space contact coordinates?
                let p_object_a = oa.position().inverse() * contact.world1;
                let p_object_b = ob.position().inverse() * contact.world2;

                if let Some(action) = action {
                    match action {
                        interaction::Action::PreventOverlap { rotate_a, rotate_b } => {
                            let filter_a = filter.filter(entity_a);
                            let filter_b = filter.filter(entity_b);

                            // Try to resolve the overlap with a constraint
                            let constraint = Constraint {
                                def: constraint::Def::Contact {
                                    normal: contact.normal.unwrap(),
                                    margin: 0.2,
                                    p_object_a,
                                    p_object_b,
                                },
                                stiffness: 1.0,
                                entity_a,
                                entity_b,
                                vars_a: constraint::Vars {
                                    p: filter_a,
                                    angle: rotate_a && filter_a
                                },
                                vars_b: constraint::Vars {
                                    p: filter_b,
                                    angle: rotate_b && filter_b
                                },
                            };
                            constraints.add(constraint);
                        }
                    }
                }

                // Record the collision event
                let info_a = interaction::EntityInfo {
                    entity: entity_a,
                    pos_object: p_object_a,
                    vel: velocity.get(entity_a).map(|v| v.0),
                };
                let info_b = interaction::EntityInfo {
                    entity: entity_b,
                    pos_object: p_object_b,
                    vel: velocity.get(entity_b).map(|v| v.0),
                };
                let event = interaction::Event {
                    a: info_a,
                    b: info_b,
                    // TODO: What is the difference between `world1` and `world2` here?
                    pos: contact.world1,
                    normal: contact.normal.unwrap(),
                };
                interactions.0.push(event);
            }
        }
    }
}

struct SolveConstraintsSys;

impl<'a> System<'a> for SolveConstraintsSys {
    type SystemData = (
        Fetch<'a, Constraints>,
        ReadStorage<'a, InvMass>,
        ReadStorage<'a, InvAngularMass>,
        WriteStorage<'a, Position>,
        WriteStorage<'a, Orientation>,
    );

    #[cfg_attr(rustfmt, rustfmt_skip)] // rustfmt bug
    fn run(
        &mut self,
        (
            constraints,
            inv_mass,
            inv_angular_mass,
            mut position,
            mut orientation,
        ): Self::SystemData
    ) {
        let num_iterations = 20;

        for _ in 0..num_iterations {
            for c in &constraints.0 {
                let (p_new_a, p_new_b) = {
                    // Set up input for constraint solving
                    let x = |entity| {
                        constraint::Position {
                            p: position.get(entity).unwrap().0,
                            angle: normalize_angle(orientation.get(entity).unwrap().0),
                        }
                    };
                    let m = |entity| {
                        constraint::Mass {
                            inv: inv_mass.get(entity).map(|m| m.0).unwrap_or(0.0),
                            inv_angular: inv_angular_mass.get(entity).map(|m| m.0).unwrap_or(0.0),
                        }
                    };

                    let x_a = x(c.entity_a);
                    let x_b = x(c.entity_b);
                    let m_a = m(c.entity_a);
                    let m_b = m(c.entity_b);

                    constraint::solve_for_position(
                        &c.def,
                        c.stiffness,
                        &x_a,
                        &x_b,
                        &m_a.zero_out_constants(&c.vars_a),
                        &m_b.zero_out_constants(&c.vars_b),
                    )
                };

                if c.vars_a.p {
                    position.insert(c.entity_a, Position(p_new_a.p));
                }
                if c.vars_b.p {
                    position.insert(c.entity_b, Position(p_new_b.p));
                }

                if c.vars_a.angle {
                    orientation.insert(c.entity_a, Orientation(normalize_angle(p_new_a.angle)));
                }
                if c.vars_b.angle {
                    orientation.insert(c.entity_b, Orientation(normalize_angle(p_new_b.angle)));
                }
            }
        }
    }
}

struct IntegrateVelocitySys;

impl<'a> System<'a> for IntegrateVelocitySys {
    type SystemData = (
        Fetch<'a, GameInfo>,
        Filter<'a>,
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
            filter,
            velocity,
            angular_velocity,
            mut position,
            mut orientation
        ): Self::SystemData
    ) {
        let dt = game_info.tick_duration_secs();

        for (_, velocity, position) in (
            filter.join(),
            &velocity,
            &mut position,
        ).join()
        {
            position.0 += velocity.0 * dt;
        }

        for (_, angular_velocity, orientation) in (
            filter.join(),
            &angular_velocity,
            &mut orientation,
        ).join()
        {
            orientation.0 += angular_velocity.0 * dt;
        }
    }
}

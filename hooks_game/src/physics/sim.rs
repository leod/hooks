use std::f32;

use nalgebra::{dot, norm, zero, Point2, Vector2};
use specs::prelude::*;

use hooks_util::{profile, stats};

use defs::GameInfo;
use entity::{self, Active};
use registry::Registry;
use repl;

use physics::collision::CollisionWorld;
use physics::constraint::Constraint;
use physics::{collision, constraint, interaction, AngularVelocity, Drag, Dynamic, Friction,
              InvAngularMass, InvMass, Orientation, Position, Update, Velocity};

pub fn register(reg: &mut Registry) {
    reg.component::<OldPosition>();
    reg.component::<OldOrientation>();
    reg.component::<Force>();

    reg.resource(InteractionEvents(Vec::new()));
    reg.resource(Constraints(Vec::new()));
}

pub const NUM_ITERATIONS: usize = 20;
pub const CONTACT_MARGIN: f32 = 1.0;

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
        self.dynamic.get(entity).is_some() && self.active.get(entity).is_some() &&
            self.update.get(entity).is_some()
    }
}

//const JOINT_MIN_DISTANCE: f32 = 0.001;
const MIN_SPEED: f32 = 0.01;

#[derive(Component, PartialEq, Clone, Debug)]
#[storage(VecStorage)]
struct OldPosition(Point2<f32>);

#[derive(Component, PartialEq, Clone, Debug)]
#[storage(VecStorage)]
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

/// Setup for additional steps to run in physics tick.
#[derive(Default)]
pub struct RunSetup {
    systems: Vec<Box<System>>,
}

impl RunSetup {
    pub fn new() -> Setup {
        Default::default()
    }

    pub fn add_system<T: System>(&mut self, system: T) {
        systems.push(Box::new(system));
    }
}

/// Stores the state necessary to run a simulation.
pub struct Run {
    setup: RunSetup,
    collision_update_sys: collision::UpdateSys,
}

impl Run {
    pub fn new(world: &mut World, setup: RunSetup) -> Run {
        Run {
            setup,
            collision_update_sys: collision::UpdateSys::new(world),
        }
    }

    pub fn run(&mut self, world: &World) {
        profile!("physics");

        PrepareSys.run_now(&world.res);

        for system in &self.setup.systems {
            system.run_now(&world.res);
        }

        collision::MaintainSys.run_now(&world.res);
        self.collision_update_sys.run_now(&world.res);

        FrictionForceSys.run_now(&world.res);
        DragForceSys.run_now(&world.res);
        //JointForceSys.run_now(&world.res);
        IntegrateForceSys.run_now(&world.res);
        SavePositionSys.run_now(&world.res);
        IntegrateVelocitySys.run_now(&world.res);
        HandleContactsSys.run_now(&world.res);
        SolveConstraintsSys.run_now(&world.res);
        CorrectVelocitySys.run_now(&world.res);

        world.write_resource::<Constraints>().0.clear();
    }

    pub fn run_interaction_events(&self, world: &World) -> Result<(), repl::Error> {
        let mut interactions = world.write_resource::<InteractionEvents>();
        for event in interactions.0.drain(..) {
            interaction::run(world, &event)?;
        }
        Ok(())
    }
}

pub fn normalize_angle(angle: f32) -> f32 {
    angle % (2.0 * f32::consts::PI)
    //angle
    //angle - 2.0 * f32::consts::PI * ((angle + f32::consts::PI) / (2.0 * f32::consts::PI)).floor()
}

#[derive(Component)]
#[storage(VecStorage)]
struct Force(Vector2<f32>);

struct PrepareSys;

impl<'a> System<'a> for PrepareSys {
    type SystemData = (Entities<'a>, Dynamic<'a>, WriteStorage<'a, Force>);

    fn run(&mut self, (entities, dynamic, mut force): Self::SystemData) {
        for (entity, _) in (&*entities, dynamic.join()).join() {
            force.insert(entity, Force(zero()));
        }
    }
}

struct FrictionForceSys;

impl<'a> System<'a> for FrictionForceSys {
    type SystemData = (
        Fetch<'a, GameInfo>,
        Filter<'a>,
        ReadStorage<'a, InvMass>,
        ReadStorage<'a, Friction>,
        WriteStorage<'a, Velocity>,
    );

    fn run(&mut self, (game_info, filter, inv_mass, friction, mut velocity): Self::SystemData) {
        let dt = game_info.tick_duration_secs();

        for (_, inv_mass, friction, velocity) in
            (filter.join(), &inv_mass, &friction, &mut velocity).join()
        {
            let speed = norm(&velocity.0);

            if speed < MIN_SPEED {
                velocity.0 = zero();
            } else {
                let force = -velocity.0 / speed * inv_mass.0 * friction.0;
                velocity.0 += force * dt;

                if dot(&velocity.0, &force) >= 0.0 {
                    velocity.0 = zero();
                }
            }
        }
    }
}

struct DragForceSys;

impl<'a> System<'a> for DragForceSys {
    type SystemData = (
        Fetch<'a, GameInfo>,
        Filter<'a>,
        ReadStorage<'a, Drag>,
        WriteStorage<'a, Velocity>,
    );

    fn run(&mut self, (game_info, filter, drag, mut velocity): Self::SystemData) {
        let dt = game_info.tick_duration_secs();

        for (_, drag, velocity) in (filter.join(), &drag, &mut velocity).join() {
            let speed = norm(&velocity.0);

            if speed < MIN_SPEED {
                velocity.0 = zero();
            } else {
                let force = -velocity.0 * drag.0;
                velocity.0 += force * dt;

                if dot(&velocity.0, &force) >= 0.0 {
                    velocity.0 = zero();
                }
            }
        }
    }
}

/*struct JointForceSys;

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
}*/

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
            old_position.insert(entity, OldPosition(position.0));
        }
        for (entity, _, orientation) in (&*entities, filter.join(), &orientation).join() {
            old_orientation.insert(entity, OldOrientation(orientation.0));
        }
    }
}

struct CorrectVelocitySys;

#[derive(SystemData)]
struct CorrectVelocityData<'a> {
    game_info: Fetch<'a, GameInfo>,

    filter: Filter<'a>,
    position: ReadStorage<'a, Position>,
    old_position: ReadStorage<'a, OldPosition>,
    orientation: ReadStorage<'a, Orientation>,
    old_orientation: ReadStorage<'a, OldOrientation>,

    velocity: WriteStorage<'a, Velocity>,
    angular_velocity: WriteStorage<'a, AngularVelocity>,
}

impl<'a> System<'a> for CorrectVelocitySys {
    type SystemData = CorrectVelocityData<'a>;

    #[cfg_attr(rustfmt, rustfmt_skip)] // rustfmt bug
    fn run(&mut self, mut data: Self::SystemData) {
        profile!("correct velocity");

        let dt = data.game_info.tick_duration_secs();

        for (_, position, old_position, velocity) in
            (data.filter.join(), &data.position, &data.old_position, &mut data.velocity).join()
        {
            velocity.0 = (position.0 - old_position.0) / dt;
        }
        for (_, orientation, old_orientation, angular_velocity) in
            (data.filter.join(), &data.orientation, &data.old_orientation, &mut data.angular_velocity).join()
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
                let positive = ang_velocity.0.is_sign_positive();
                let signum = ang_velocity.0.signum();
                ang_velocity.0 -= 100.0 * signum * dt;
                if ang_velocity.0.is_sign_positive() != positive {
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
        ReadStorage<'a, repl::Id>,
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
            repl_id,
            velocity,
        ): Self::SystemData
    ) {
        profile!("handle contacts");

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

                // Let's not have a player's entities collide with each other just yet
                match (repl_id.get(entity_a), repl_id.get(entity_b)) {
                    (Some(&repl::Id((owner_a, _))), Some(&repl::Id((owner_b, _))))
                        if owner_a == owner_b => continue,
                    _ => {},
                }

                let action = interaction::get_action(
                    &interaction_handlers,
                    &meta,
                    entity_a,
                    entity_b
				);

                // TODO: Easier way to get object-space contact coordinates?
                let object_pos_a = oa.position().inverse() * contact.world1;
                let object_pos_b = ob.position().inverse() * contact.world2;

                if let Some(action) = action {
                    match action {
                        interaction::Action::PreventOverlap { rotate_a, rotate_b } => {
                            // Try to resolve the overlap with a constraint
                            let constraint = Constraint {
                                def: constraint::Def::Contact {
                                    normal: contact.normal.unwrap(),
                                    margin: CONTACT_MARGIN,
                                    object_pos_a,
                                    object_pos_b,
                                },
                                stiffness: 1.0,
                                entity_a,
                                entity_b,
                                vars_a: constraint::Vars {
                                    pos: true,
                                    angle: rotate_a,
                                },
                                vars_b: constraint::Vars {
                                    pos: true,
                                    angle: rotate_b,
                                },
                            };
                            constraints.add(constraint);
                        }
                    }
                }

                // Record the collision event
                let info_a = interaction::EntityInfo {
                    entity: entity_a,
                    object_pos: object_pos_a,
                    vel: velocity.get(entity_a).map(|v| v.0),
                };
                let info_b = interaction::EntityInfo {
                    entity: entity_b,
                    object_pos: object_pos_b,
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
        Filter<'a>,
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
            filter,
            inv_mass,
            inv_angular_mass,
            mut position,
            mut orientation,
        ): Self::SystemData
    ) {
        profile!("solve");

        stats::record("constraints", constraints.0.len() as f32);

        for _ in 0..NUM_ITERATIONS {
            for c in &constraints.0 {
                let (p_new_a, p_new_b) = {
                    // Set up input for constraint solving
                    let m = |entity| {
                        constraint::Mass {
                            inv: inv_mass.get(entity).map(|m| m.0).unwrap_or(0.0),
                            inv_angular: inv_angular_mass.get(entity).map(|m| m.0).unwrap_or(0.0),
                        }
                    };

                    // TODO: repl unwrap
                    let x_a = constraint::Pose::from_entity(
                        &position,
                        &orientation,
                        c.entity_a
                    ).unwrap();
                    let x_b = constraint::Pose::from_entity(
                        &position,
                        &orientation,
                        c.entity_b
                    ).unwrap();
                    let m_a = m(c.entity_a);
                    let m_b = m(c.entity_b);
                    let vars_a = if filter.filter(c.entity_a) {
                        c.vars_a.clone()
                    } else {
                        constraint::Vars::none()
                    };
                    let vars_b = if filter.filter(c.entity_b) {
                        c.vars_b.clone()
                    } else {
                        constraint::Vars::none()
                    };

                    constraint::solve_for_position(
                        &c.def,
                        c.stiffness,
                        &x_a,
                        &x_b,
                        &m_a.zero_out_constants(&vars_a),
                        &m_b.zero_out_constants(&vars_b),
                    )
                };

                if c.vars_a.pos {
                    position.insert(c.entity_a, Position(p_new_a.pos));
                }
                if c.vars_b.pos {
                    position.insert(c.entity_b, Position(p_new_b.pos));
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
            //debug!("moving {}*{}={}", velocity.0, dt, velocity.0 * dt);
            position.0 += velocity.0 * dt;
        }

        for (_, angular_velocity, orientation) in (
            filter.join(),
            &angular_velocity,
            &mut orientation,
        ).join()
        {
            //debug!("rotating {}*{}={}", angular_velocity.0, dt, angular_velocity.0 * dt);
            orientation.0 += angular_velocity.0 * dt;
        }
    }
}

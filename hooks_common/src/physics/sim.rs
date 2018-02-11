use nalgebra::{norm, zero, Point2, Vector2};

use specs::{self, Entities, Entity, Fetch, FetchMut, Join, ReadStorage, RunNow, System,
            VecStorage, World, WriteStorage};

use hooks_util::profile;

use defs::GameInfo;
use entity::Active;
use physics::{collision, interaction, Dynamic, Friction, Joints, Mass, Position, Velocity};
use physics::collision::CollisionWorld;
use registry::Registry;

pub fn register(reg: &mut Registry) {
    reg.component::<Collided>();

    reg.component::<OldPosition>();
    reg.component::<Force>();

    reg.resource(Interactions(Vec::new()));
}

const JOINT_MIN_DISTANCE: f32 = 0.01;
const MIN_SPEED: f32 = 0.01;
const FRICTION: f32 = 0.8;

/// Tag component for debugging visually
#[derive(Component)]
#[component(BTreeStorage)]
pub struct Collided {
    pub other: specs::Entity,
}

/// Resource to store the interactions that were detected in a time step.
struct Interactions(Vec<(Entity, Entity, Point2<f32>)>);

/// For now, it seems that putting the whole physics simulation into a set of systems would be
/// clumsy. For example, to resolve collisions with impulses, we might need to iterate some systems
/// multiple times. However, systems don't seem to be easily composable with specs.
///
/// Thus, we are putting the simulation into this function.
pub fn run(world: &World) {
    profile!("physics");

    ClearSys.run_now(&world.res);

    collision::MaintainSys.run_now(&world.res);

    FrictionForceSys.run_now(&world.res);
    JointForceSys.run_now(&world.res);
    ApplyForceSys.run_now(&world.res);

    PredictSys.run_now(&world.res);
    collision::UpdateSys.run_now(&world.res);
    ApplySys.run_now(&world.res);

    let interactions = world.read_resource::<Interactions>().0.clone();

    for &(entity_a, entity_b, pos) in &interactions {
        interaction::run(world, entity_a, entity_b, pos);
    }
}

#[derive(Component)]
#[component(VecStorage)]
struct OldPosition(Point2<f32>);

#[derive(Component)]
#[component(VecStorage)]
struct Force(Vector2<f32>);

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
        for (_, _, _, velocity, force) in
            (&active, &dynamic, &friction, &mut velocity, &mut force).join()
        {
            let speed = norm(&velocity.0);

            if speed < MIN_SPEED {
                velocity.0 = zero();
            } else {
                force.0 -= velocity.0 * FRICTION;
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
        for (_, _, joints, position_a, force) in
            (&active, &dynamic, &joints, &positions, &mut force).join()
        {
            for &(entity_b, ref joint) in &joints.0 {
                if active.get(entity_b).is_none() {
                    // Both endpoints of the joint need to be active
                    continue;
                }

                // TODO: Should we lazily remove joints whose endpoint entity no longer exists?
                //       => Probably better to do it in a `RemovalSys`. We don't need this
                //          currently as all joints are created in "immediate mode".

                let position_b = positions.get(entity_b).unwrap();

                let delta = position_b.0 - position_a.0;
                let distance = norm(&delta);
                let t = distance - joint.resting_length;

                if t >= JOINT_MIN_DISTANCE {
                    force.0 += joint.stiffness * t * delta / distance;
                }
            }
        }
    }
}

struct ApplyForceSys;

impl<'a> System<'a> for ApplyForceSys {
    type SystemData = (
        Fetch<'a, GameInfo>,
        ReadStorage<'a, Active>,
        ReadStorage<'a, Mass>,
        ReadStorage<'a, Dynamic>,
        ReadStorage<'a, Force>,
        WriteStorage<'a, Velocity>,
    );

    fn run(&mut self, (game_info, active, mass, dynamic, force, mut velocity): Self::SystemData) {
        let dt = game_info.tick_duration_secs() as f32;

        for (_, _, mass, force, velocity) in
            (&active, &dynamic, &mass, &force, &mut velocity).join()
        {
            velocity.0 += force.0 / mass.0 * dt;
            //velocity.0 *= 0.9;
        }
    }
}

struct PredictSys;

impl<'a> System<'a> for PredictSys {
    type SystemData = (
        Fetch<'a, GameInfo>,
        Entities<'a>,
        ReadStorage<'a, Active>,
        ReadStorage<'a, Dynamic>,
        WriteStorage<'a, Velocity>,
        WriteStorage<'a, Position>,
        WriteStorage<'a, OldPosition>,
    );

    #[cfg_attr(rustfmt, rustfmt_skip)] // rustfmt bug
    fn run(
        &mut self,
        (
            game_info,
            entities,
            active,
            dynamic,
            mut velocity,
            mut position,
            mut old_position,
        ): Self::SystemData
    ) {
        let dt = game_info.tick_duration_secs() as f32;

        for (entity, _, _, position) in (&*entities, &active, &dynamic, &position).join() {
            old_position.insert(entity, OldPosition(position.0));
        }

        for (velocity, _, _, position) in (&mut velocity, &active, &dynamic, &mut position).join() {
            // TODO: Only mutate position when velocity is non-zero
            position.0 += velocity.0 * dt;
        }
    }
}

struct ApplySys;

impl<'a> System<'a> for ApplySys {
    type SystemData = (
        Fetch<'a, GameInfo>,
        Fetch<'a, CollisionWorld>,
        FetchMut<'a, Interactions>,
        ReadStorage<'a, Active>,
        ReadStorage<'a, Dynamic>,
        WriteStorage<'a, Velocity>,
        WriteStorage<'a, Position>,
        WriteStorage<'a, OldPosition>,
        WriteStorage<'a, Collided>,
    );

    #[cfg_attr(rustfmt, rustfmt_skip)] // rustfmt bug
    fn run(
        &mut self,
        (
            game_info,
            collision_world,
            mut interactions,
            active,
            dynamic,
            mut velocity,
            mut position,
            old_position,
            mut collided,
        ): Self::SystemData
    ) {
        let dt = game_info.tick_duration_secs() as f32;

        for (oa, ob, gen) in collision_world.contact_pairs() {
            let mut contacts = Vec::new();
            gen.contacts(&mut contacts);

            for contact in &contacts {
                let entity_a = *oa.data();
                let entity_b = *ob.data();

                let dynamic_a = dynamic.get(entity_a).is_some();
                let dynamic_b = dynamic.get(entity_b).is_some();

                // Only active objects should be involved in the collision pipeline
                assert!(active.get(entity_a).is_some());
                assert!(active.get(entity_b).is_some());

                /*debug!(
                    "contact {} {} with depth {}",
                    oa.handle().uid(), ob.handle().uid(), contact.depth
                );*/

                fn resolve(dt: f32, n: &Vector2<f32>, depth: f32, v: &mut Velocity) {
                    //let t = depth.min(dot(&v.0, &n));
                    let t = depth;
                    //debug!("resolving {:?} with {}: {:?}", v.0, t / dt, n * t / dt);
                    v.0 += n * t / dt;
                    //debug!("-> {:?}", v.0);
                }

                if dynamic_a && !dynamic_b {
                    //velocity.get_mut(a).unwrap().0 -= contact.normal * contact.depth / dt;
                    resolve(
                        dt,
                        &contact.normal,
                        contact.depth,
                        velocity.get_mut(entity_a).unwrap(),
                    );
                } else if !dynamic_a && dynamic_b {
                    resolve(
                        dt,
                        &contact.normal,
                        contact.depth,
                        velocity.get_mut(entity_b).unwrap(),
                    );
                } else {
                    unimplemented!();
                }

                collided.insert(entity_a, Collided { other: entity_b });
                collided.insert(entity_b, Collided { other: entity_a });

                // TODO: Fix this position
                let pos = contact.world1;

                interactions.0.push((entity_a, entity_b, pos));
            }
        }

        for (_, _, position, old_position) in
            (&active, &dynamic, &mut position, &old_position).join() {
            // TODO: Only mutate position when position has changed
            position.0 = old_position.0;
        }

        for (_, _, velocity, position) in (&active, &dynamic, &mut velocity, &mut position).join()
        {
            // TODO: Only mutate position when velocity is non-zero
            position.0 += velocity.0 * dt;
        }
    }
}

struct ClearSys;

impl<'a> System<'a> for ClearSys {
    type SystemData = (
        FetchMut<'a, Interactions>,
        Entities<'a>,
        ReadStorage<'a, Dynamic>,
        WriteStorage<'a, Force>,
        WriteStorage<'a, Collided>,
    );

    fn run(
        &mut self,
        (mut interactions, entities, dynamic, mut force, mut collided): Self::SystemData,
    ) {
        for (entity, _) in (&*entities, &dynamic).join() {
            force.insert(entity, Force(zero()));
            collided.remove(entity);
        }

        interactions.0.clear();
    }
}

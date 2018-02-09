use nalgebra::{norm, zero, Point2, Vector2};

use specs::{self, Entities, Fetch, FetchMut, Join, ReadStorage, RunNow, System, VecStorage, World,
            WriteStorage};

use hooks_util::profile;

use defs::GameInfo;
use physics::{collision, Dynamic, Friction, Joints, Mass, Position, Velocity};
use physics::collision::CollisionWorld;
use registry::Registry;

pub fn register(reg: &mut Registry) {
    reg.component::<Collided>();

    reg.component::<OldPosition>();
    reg.component::<Force>();
}

const JOINT_MIN_DISTANCE: f32 = 0.01;
const MIN_SPEED: f32 = 0.01;
const FRICTION: f32 = 0.3;

/// Tag component for debugging visually
#[derive(Component)]
#[component(BTreeStorage)]
pub struct Collided {
    pub other: specs::Entity,
}

/// For now, it seems that putting the whole physics simulation into a set of systems would be
/// clumsy. For example, to resolve collisions with impulses, we might need to iterate some systems
/// multiple times. However, systems don't seem to be easily composable with specs.
///
/// Thus, we are putting the simulation into this function.
pub fn run(world: &World) {
    profile!("physics");

    ClearSys.run_now(&world.res);
    collision::CreateObjectSys.run_now(&world.res);
    // TODO: Remove entities from `ncollide`

    FrictionForceSys.run_now(&world.res);
    JointForceSys.run_now(&world.res);
    ApplyForceSys.run_now(&world.res);

    PredictSys.run_now(&world.res);
    collision::UpdateSys.run_now(&world.res);
    ApplySys.run_now(&world.res);
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
        ReadStorage<'a, Dynamic>,
        ReadStorage<'a, Friction>,
        WriteStorage<'a, Velocity>,
        WriteStorage<'a, Force>,
    );

    fn run(&mut self, (dynamic, friction, mut velocity, mut force): Self::SystemData) {
        for (_, _, velocity, force) in (&dynamic, &friction, &mut velocity, &mut force).join() {
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
        Fetch<'a, GameInfo>,
        ReadStorage<'a, Dynamic>,
        ReadStorage<'a, Joints>,
        ReadStorage<'a, Position>,
        WriteStorage<'a, Force>,
    );

    fn run(&mut self, (game_info, dynamic, joints, positions, mut force): Self::SystemData) {
        let dt = game_info.tick_duration_secs() as f32;

        for (_, joints, position_a, force) in (&dynamic, &joints, &positions, &mut force).join() {
            for &(entity_b, ref joint) in &joints.0 {
                // TODO: Should we lazily remove joints whose endpoint entity no longer exists?

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
        ReadStorage<'a, Mass>,
        ReadStorage<'a, Dynamic>,
        ReadStorage<'a, Force>,
        WriteStorage<'a, Velocity>,
    );

    fn run(&mut self, (game_info, mass, dynamic, force, mut velocity): Self::SystemData) {
        let dt = game_info.tick_duration_secs() as f32;

        for (_, mass, force, velocity) in (&dynamic, &mass, &force, &mut velocity).join() {
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
        WriteStorage<'a, Velocity>,
        WriteStorage<'a, Position>,
        WriteStorage<'a, OldPosition>,
        ReadStorage<'a, Dynamic>,
    );

    #[cfg_attr(rustfmt, rustfmt_skip)] // rustfmt bug
    fn run(
        &mut self,
        (
            game_info,
            entities,
            mut velocity,
            mut position,
            mut old_position,
            dynamic
        ): Self::SystemData
    ) {
        let dt = game_info.tick_duration_secs() as f32;

        for (entity, position, _dynamic) in (&*entities, &position, &dynamic).join() {
            old_position.insert(entity, OldPosition(position.0));
        }

        for (velocity, position, _dynamic) in (&mut velocity, &mut position, &dynamic).join() {
            // TODO: Only mutate position when velocity is non-zero
            position.0 += velocity.0 * dt;
        }
    }
}

struct ApplySys;

impl<'a> System<'a> for ApplySys {
    type SystemData = (
        Fetch<'a, GameInfo>,
        FetchMut<'a, CollisionWorld>,
        WriteStorage<'a, Velocity>,
        WriteStorage<'a, Position>,
        WriteStorage<'a, OldPosition>,
        WriteStorage<'a, Collided>,
        ReadStorage<'a, Dynamic>,
    );

    #[cfg_attr(rustfmt, rustfmt_skip)] // rustfmt bug
    fn run(
        &mut self,
        (
            game_info,
            collision_world,
            mut velocity,
            mut position,
            old_position,
            mut collided,
            dynamic
        ): Self::SystemData
    ) {
        let dt = game_info.tick_duration_secs() as f32;

        for (oa, ob, gen) in collision_world.contact_pairs() {
            let mut contacts = Vec::new();
            gen.contacts(&mut contacts);

            for contact in &contacts {
                let a = *oa.data();
                let b = *ob.data();

                let a_dynamic = dynamic.get(a).is_some();
                let b_dynamic = dynamic.get(b).is_some();

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

                if a_dynamic && !b_dynamic {
                    //velocity.get_mut(a).unwrap().0 -= contact.normal * contact.depth / dt;
                    resolve(dt, &contact.normal, contact.depth, velocity.get_mut(a).unwrap());
                } else if !a_dynamic && b_dynamic {
                    resolve(dt, &contact.normal, contact.depth, velocity.get_mut(b).unwrap());
                } else {
                    unimplemented!();
                }

                collided.insert(a, Collided { other: b });
                collided.insert(b, Collided { other: a });
            }
        }

        for (position, old_position, _dynamic) in (&mut position, &old_position, &dynamic).join() {
            // TODO: Only mutate position when position has changed
            position.0 = old_position.0;
        }

        for (velocity, position, _dynamic) in
            (&mut velocity, &mut position, &dynamic).join()
        {
            // TODO: Only mutate position when velocity is non-zero
            position.0 += velocity.0 * dt;
        }
    }
}

struct ClearSys;

impl<'a> System<'a> for ClearSys {
    type SystemData = (
        Entities<'a>,
        ReadStorage<'a, Dynamic>,
        WriteStorage<'a, Force>,
        WriteStorage<'a, Collided>,
    );

    fn run(&mut self, (entities, dynamic, mut force, mut collided): Self::SystemData) {
        for (entity, _) in (&*entities, &dynamic).join() {
            force.insert(entity, Force(zero()));
            collided.remove(entity);
        }
    }
}

use nalgebra::{Point2, Vector2};

use specs::{self, Entities, Fetch, FetchMut, Join, ReadStorage, RunNow, System, VecStorage, World,
            WriteStorage};

use hooks_util::profile;

use defs::GameInfo;
use physics::{collision, Dynamic, Position, Velocity};
use physics::collision::CollisionWorld;
use registry::Registry;

pub fn register(reg: &mut Registry) {
    reg.component::<OldPosition>();
    reg.component::<Collided>();
}

/// Tag component for debuggin visually
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

    PredictSys.run_now(&world.res);
    collision::UpdateSys.run_now(&world.res);
    ApplySys.run_now(&world.res);
}

#[derive(Component)]
#[component(VecStorage)]
struct OldPosition(Point2<f32>);

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
                let a = oa.data;
                let b = ob.data;

                let a_dynamic = dynamic.get(a).is_some();
                let b_dynamic = dynamic.get(b).is_some();

                fn resolve(dt: f32, n: &Vector2<f32>, depth: f32, v: &mut Velocity) {
                    //let t = depth.min(dot(&v.0, &n));
                    let t = depth;
                    v.0 -= n * t / dt;
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
        WriteStorage<'a, Collided>,
    );

    fn run(&mut self, (entities, dynamic, mut collided): Self::SystemData) {
        for (entity, _) in (&*entities, &dynamic).join() {
            collided.remove(entity);
        }
    }
}

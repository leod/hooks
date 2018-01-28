use nalgebra::{Point2, Vector2};

use specs::{Entities, Fetch, Join, ReadStorage, RunNow, System, VecStorage, World, WriteStorage};

use defs::GameInfo;
use physics::{collision, Dynamic, Position, Velocity};
use physics::collision::{CollisionWorld, Cuboid};
use registry::Registry;

pub fn register(reg: &mut Registry) {
    reg.component::<OldPosition>();
}

/// For now, it seems that putting the whole physics simulation into a set of systems would be
/// clumsy. For example, to resolve collisions with impulses, we might need to iterate some systems
/// multiple times. However, systems don't seem to be easily composable with specs.
///
/// Thus, we are putting the simulation into this function.
pub fn run(world: &World) {
    collision::CreateObjectSys.run_now(&world.res);
    collision::UpdateSys.run_now(&world.res);
    Step.run_now(&world.res);

    collision::UpdateSys.run_now(&world.res);

    let collision_world = world.read_resource::<CollisionWorld>();

    for (oa, ob, gen) in collision_world.contact_pairs() {
        //debug!("{} colliding with {}", oa.uid, ob.uid);
        //debug!("{:?}", oa.position);
        //debug!("{:?}", ob.position);
        //debug!("{:?}", oa.shape.as_shape::<Cuboid<Vector2<f32>>>().unwrap());
        //debug!("{:?}", ob.shape.as_shape::<Cuboid<Vector2<f32>>>().unwrap());

        let mut contacts = Vec::new();
        gen.contacts(&mut contacts);

        for contact in &contacts {
            debug!("contact {:?}", contact);
            //assert!(false);
        }
    }
}

#[derive(Component)]
#[component(VecStorage)]
struct OldPosition(Point2<f32>);

struct Step;

impl<'a> System<'a> for Step {
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
            old_position.insert(entity, OldPosition(position.0.clone()));
        }

        for (velocity, position, old_position, _dynamic) in
            (&mut velocity, &mut position, &mut old_position, &dynamic).join()
        {
            // TODO: Only mutate position when velocity is non-zero
            position.0 += velocity.0 * dt;
        }
    }
}

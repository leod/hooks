use nalgebra::Point2;

use specs::{RunNow, System, VecStorage, World};

use physics::collision;
use registry::Registry;

pub fn register(reg: &mut Registry) {
    reg.component::<OldPosition>();
}

#[derive(Component)]
#[component(VecStorage)]
struct OldPosition(Point2<f32>);

/// For now, it seems that putting the whole physics simulation into a set of systems would be
/// clumsy. For example, to resolve collisions with impulses, we might need to iterate some systems
/// multiple times. However, systems don't seem to be easily composable with specs.
///
/// Thus, we are putting the simulation into this function.
pub fn run(world: &World) {
    collision::CreateObjectSys.run_now(&world.res);
    collision::UpdateSys.run_now(&world.res);
}

struct Step;

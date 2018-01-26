use specs::World;

use physics::collision;

// For now, it seems that putting the whole physics simulation into a set of systems would be
// clumsy. For example, to resolve collisions with impulses, we might need to iterate some systems
// multiple times. However, systems don't seem to be easily composable with specs.

pub fn run(world: &World) {}

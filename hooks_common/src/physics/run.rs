use specs::System;

use physics::collision;

pub struct RunSys;

impl<'a> System<'a> for RunSys {
    type SystemData = ();

    fn run(&mut self, (): Self::SystemData) {}
}

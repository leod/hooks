use nalgebra::Point2;

use specs::{BTreeStorage, Fetch, System, WriteStorage};

use defs::GameInfo;
use game::ComponentType;
use physics::{Orientation, Position};
use registry::Registry;
use repl::entity;

pub fn register(reg: &mut Registry) {
    entity::register_type(
        "test",
        vec![ComponentType::Position, ComponentType::Orientation],
        |builder| builder,
        reg,
    );
}

pub mod auth {
    use specs::Join;

    use super::*;

    pub fn register(reg: &mut Registry) {
        super::register(reg);

        reg.component::<Test>();
        reg.tick_system(TickSys, "test", &[]);

        entity::add_ctor(
            "test",
            |builder| {
                builder
                    .with(Orientation { angle: 0.0 })
                    .with(Test(0.0))
            },
            reg,
        );
    }

    #[derive(Component)]
    #[component(BTreeStorage)]
    struct Test(f64);

    struct TickSys;

    impl<'a> System<'a> for TickSys {
        type SystemData = (
            Fetch<'a, GameInfo>,
            WriteStorage<'a, Position>,
            WriteStorage<'a, Test>,
        );

        fn run(&mut self, (game_info, mut position, mut test): Self::SystemData) {
            for (position, test) in (&mut position, &mut test).join() {
                position.pos.x = (test.0.sin() * 100.0) as f32;
                test.0 += game_info.tick_duration_secs();
            }
        }
    }
}

use nalgebra::Point2;

use entity;
use game::ComponentType;
use physics::constraint::{self, Constraint};
use physics::sim::Constraints;
use physics::{Dynamic, InvAngularMass, InvMass, Orientation};
use registry::Registry;
use repl;

pub fn register(reg: &mut Registry) {
    repl::entity::register_class(
        reg,
        "test",
        &[ComponentType::Position, ComponentType::Orientation],
        |builder| builder,
    );
}

pub mod auth {
    use specs::prelude::*;
    use specs::storage::BTreeStorage;

    use super::*;

    pub fn register(reg: &mut Registry) {
        super::register(reg);

        reg.component::<Test>();
        reg.tick_system(TickSys, "test", &[]);

        entity::add_ctor(reg, "test", |builder| {
            builder
                .with(Orientation(0.0))
                .with(Test(0.0))
                .with(Dynamic)
                .with(InvMass(1.0))
                .with(InvAngularMass(1.0))
        });
    }

    #[derive(Component)]
    #[storage(BTreeStorage)]
    struct Test(f64);

    struct TickSys;

    impl<'a> System<'a> for TickSys {
        type SystemData = (
            Entities<'a>,
            FetchMut<'a, Constraints>,
            ReadStorage<'a, Test>,
        );

        fn run(&mut self, (entities, mut constraints, test): Self::SystemData) {
            let test_entities = (&*entities, &test)
                .join()
                .map(|(e, _)| e)
                .collect::<Vec<_>>();
            for (&a, &b) in test_entities.iter().zip(test_entities.iter().skip(1)) {
                let constraint = Constraint {
                    def: constraint::Def::Joint {
                        distance: 300.0,
                        object_pos_a: Point2::origin(),
                        object_pos_b: Point2::origin(),
                    },
                    stiffness: 1.0,
                    entity_a: a,
                    entity_b: b,
                    vars_a: constraint::Vars {
                        pos: true,
                        angle: false,
                    },
                    vars_b: constraint::Vars {
                        pos: true,
                        angle: false,
                    },
                };
                constraints.add(constraint);
            }
        }
    }
}

use nalgebra::{Point2, Vector2};

use defs::GameInfo;
use game::ComponentType;
use physics::{AngularVelocity, Dynamic, InvAngularMass, InvMass, Orientation, Position, Velocity};
use physics::constraint::{self, Constraint};
use physics::sim::Constraints;
use registry::Registry;
use entity;
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
    use specs::{BTreeStorage, Entities, Fetch, FetchMut, Join, ReadStorage, System, WriteStorage};

    use super::*;

    pub fn register(reg: &mut Registry) {
        super::register(reg);

        reg.component::<Test>();
        reg.tick_system(TickSys, "test", &[]);

        entity::add_ctor(reg, "test", |builder| {
            builder
                .with(Orientation(0.0))
                .with(AngularVelocity(0.1))
                .with(Test(0.0))
                .with(Dynamic)
                .with(InvMass(1.0))
                .with(InvAngularMass(1.0))
        });
    }

    #[derive(Component)]
    #[component(BTreeStorage)]
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
                    entity_a: a,
                    entity_b: b,
                    vars_a: constraint::Vars {
                        p: true,
                        angle: true,
                    },
                    vars_b: constraint::Vars {
                        p: true,
                        angle: true,
                    },
                    def: constraint::Def {
                        kind: constraint::Kind::Joint { distance: 300.0 },
                        p_object_a: Point2::new(0.0, 100.0),
                        p_object_b: Point2::origin(),
                    },
                };
                constraints.add(constraint);
            }
        }
    }
}

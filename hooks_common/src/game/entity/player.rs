use specs::BTreeStorage;

use game::ComponentType;
use physics::Orientation;
use registry::Registry;
use repl::entity;

pub fn register(reg: &mut Registry) {
    reg.component::<Player>();

    entity::register_type(
        "player",
        vec![ComponentType::Position, ComponentType::Orientation],
        |builder| builder.with(Orientation { angle: 0.0 }),
        reg,
    );
}

#[derive(Component)]
#[component(BTreeStorage)]
struct Player;

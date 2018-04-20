use hooks_game;
use hooks_game::entity;
use hooks_game::game::entity::player;

use rect;

pub fn register(reg: &mut hooks_game::Registry) {
    // TODO: Get sizes from e.g. collision shapes?

    entity::add_ctor(reg, "test", |builder| {
        builder.with(rect::Draw {
            width: 200.0,
            height: 200.0,
            fill: true,
        })
    });

    entity::add_ctor(reg, "player", |builder| {
        builder.with(rect::Draw {
            width: player::WIDTH,
            height: player::HEIGHT,
            fill: false,
        })
    });

    /*entity::add_ctor(reg, "hook_segment", |builder| {
        builder.with(rect::Draw {
            width: 10.0,
            height: 3.0,
        })
    });*/
}

use hooks_common;
use hooks_common::entity;
use hooks_common::game::entity::player;

use rect;

pub fn register(reg: &mut hooks_common::Registry) {
    // TODO: Get sizes from e.g. collision shapes?

    entity::add_ctor(reg, "test", |builder| {
        builder.with(rect::Draw {
            width: 30.0,
            height: 30.0,
        })
    });

    entity::add_ctor(reg, "player", |builder| {
        builder.with(rect::Draw {
            width: player::WIDTH,
            height: player::HEIGHT,
        })
    });

    /*entity::add_ctor(reg, "hook_segment", |builder| {
        builder.with(rect::Draw {
            width: 10.0,
            height: 3.0,
        })
    });*/
}

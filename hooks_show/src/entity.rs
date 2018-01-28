use hooks_common;
use hooks_common::repl::entity;

use rect;

pub fn register(reg: &mut hooks_common::Registry) {
    entity::add_ctor(reg, "test", |builder| {
        builder.with(rect::Draw {
            width: 10.0,
            height: 10.0,
        })
    });

    entity::add_ctor(reg, "player", |builder| {
        builder.with(rect::Draw {
            width: 20.0,
            height: 20.0,
        })
    });
}

use common;
use common::repl::entity;

use show;

pub fn register(reg: &mut common::Registry) {
    entity::add_ctor(reg, "test", |builder| {
        builder.with(show::rect::Draw {
            width: 10.0,
            height: 10.0,
        })
    });

    entity::add_ctor(reg, "player", |builder| {
        builder.with(show::rect::Draw {
            width: 20.0,
            height: 20.0,
        })
    });
}

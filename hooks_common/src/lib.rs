extern crate bit_manager;
#[macro_use]
extern crate bit_manager_derive;
extern crate enet_sys;
extern crate libc;
#[macro_use]
extern crate log;
#[macro_use]
extern crate mopa;
extern crate nalgebra;
extern crate ncollide;
extern crate rand;
extern crate shred;
#[macro_use]
extern crate shred_derive;
extern crate specs;
#[macro_use]
extern crate specs_derive;
extern crate take_mut;

pub mod ordered_join;
pub mod timer;
pub mod defs;
pub mod physics;
#[macro_use]
pub mod event;
pub mod registry;
#[macro_use]
pub mod repl;
pub mod net;
pub mod game;

pub use defs::*;
pub use event::Event;
pub use registry::Registry;

fn register(reg: &mut Registry, game_info: &GameInfo) {
    reg.resource(game_info.clone());
    reg.resource(event::Sink::new());

    physics::register(reg);
    repl::register(reg);
    repl::player::register(reg);
}

pub mod auth {
    use super::*;

    pub fn register(reg: &mut Registry, game_info: &GameInfo) {
        super::register(reg, game_info);
        game::auth::register(reg);
    }
}

pub mod view {
    use super::*;

    pub fn register(reg: &mut Registry, game_info: &GameInfo) {
        super::register(reg, game_info);
        game::view::register(reg);
    }
}

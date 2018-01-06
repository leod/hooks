extern crate bincode;
extern crate bit_manager;
#[macro_use]
extern crate bit_manager_derive;
extern crate enet_sys;
extern crate libc;
#[macro_use]
extern crate mopa;
extern crate nalgebra;
extern crate ncollide;
#[macro_use]
extern crate serde;
extern crate shred;
#[macro_use]
extern crate shred_derive;
extern crate specs;
#[macro_use]
extern crate specs_derive;

mod ordered_join;
mod defs;
mod physics;
#[macro_use]
mod event;
mod repl;
mod net;

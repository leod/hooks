#[macro_use] extern crate serde;
extern crate shred;
extern crate specs;
#[macro_use] extern crate specs_derive;
extern crate nalgebra;
extern crate ncollide;
extern crate enet_sys as enet;

mod defs;
mod physics;
mod repl;
mod net;
mod ordered_join;

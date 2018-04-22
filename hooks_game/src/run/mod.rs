pub mod common;
pub mod auth;
pub mod view;

use specs::World;

pub struct State {
    pub world: World,
}

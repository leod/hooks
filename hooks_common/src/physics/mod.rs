mod defs;
pub mod collision;
pub mod interaction;
pub mod sim;

use registry::Registry;

pub use self::defs::*;

pub fn register(reg: &mut Registry) {
    defs::register(reg);
    collision::register(reg);
    interaction::register(reg);
    sim::register(reg);
}

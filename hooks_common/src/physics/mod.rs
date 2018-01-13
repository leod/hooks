mod defs;
pub mod collision;

use registry::Registry;

pub use self::defs::*;

pub fn register(reg: &mut Registry) {
    defs::register(reg);
    collision::register(reg);
}

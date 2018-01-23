mod test;
mod player;
pub mod wall;

use registry::Registry;

pub mod auth {
    use super::*;

    pub fn register(reg: &mut Registry) {
        test::auth::register(reg);
        player::register(reg);
        wall::register(reg);
    }
}

pub mod view {
    use super::*;

    pub fn register(reg: &mut Registry) {
        test::register(reg);
        player::register(reg);
        wall::register(reg);
    }
}

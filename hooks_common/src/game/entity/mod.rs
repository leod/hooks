mod test;
pub mod player;
pub mod wall;

use registry::Registry;

fn register(reg: &mut Registry) {
    player::register(reg);
    wall::register(reg);
}

pub mod auth {
    use super::*;

    pub fn register(reg: &mut Registry) {
        super::register(reg);

        test::auth::register(reg);
    }
}

pub mod view {
    use super::*;

    pub fn register(reg: &mut Registry) {
        super::register(reg);

        test::register(reg);
    }
}

mod test;

use registry::Registry;

pub mod auth {
    use super::*;

    pub fn register(reg: &mut Registry) {
        test::auth::register(reg);
    }
}

pub mod view {
    use super::*;

    pub fn register(reg: &mut Registry) {
        test::register(reg);
    }
}

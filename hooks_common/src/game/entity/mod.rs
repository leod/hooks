mod test;
pub mod hook;
pub mod player;
pub mod wall;

use registry::Registry;

// For nicer names in the component enum generated by the `snapshot` macro in `game`.
pub type HookDef = hook::Def;
pub type HookSegmentDef = hook::SegmentDef;
pub type HookState = hook::State;

fn register(reg: &mut Registry) {
    hook::register(reg);
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

use super::*;

#[derive(Component, PartialEq, Clone, BitStore)]
pub struct A {
    pub x: f32,
}

#[derive(Component, PartialEq, Clone, BitStore)]
pub struct B {
    pub x: u16,
    pub y: bool,
}

#[derive(Component, PartialEq, Clone, BitStore)]
pub struct C {
    pub x: A,
    pub y: B,
}

#[derive(Component, PartialEq, Clone, BitStore)]
pub struct D;

snapshot! {
    use super::A;
    use super::B;
    use super::C;
    use super::D;

    mod snap {
        a: A,
        b: B,
        c: C,
        d: D,
    }
}

#[test]
fn test_delta() {
}

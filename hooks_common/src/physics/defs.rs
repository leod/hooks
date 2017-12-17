use specs::{VecStorage, HashMapStorage};

use nalgebra::Point2;
use ncollide::shape::ShapeHandle2;

#[derive(Component, PartialEq, Clone, Debug, Serialize, Deserialize)]
#[component(VecStorage)]
pub struct Position {
    pub pos: Point2<f32>,
}

#[derive(Component, PartialEq, Clone, Debug, Serialize, Deserialize)]
#[component(VecStorage)]
pub struct Orientation {
    pub angle: f32,
}

#[derive(Clone, Component)]
#[component(VecStorage)]
pub struct CollisionShape {
    pub shape: ShapeHandle2<f32>,
}


use specs::{VecStorage, HashMapStorage};

use nalgebra::Point2;
use ncollide::shape::ShapeHandle2;

use repl::ReplComponent;

#[derive(Component, PartialEq, Clone, Debug, Serialize, Deserialize)]
#[component(VecStorage)]
pub struct Position {
    pub pos: Point2<f32>,
}

impl ReplComponent for Position {}

#[derive(Component, PartialEq, Clone, Debug, Serialize, Deserialize)]
#[component(VecStorage)]
pub struct Orientation {
    pub angle: f32,
}

impl ReplComponent for Orientation {}

#[derive(Clone, Component)]
#[component(VecStorage)]
pub struct CollisionShape {
    pub shape: ShapeHandle2<f32>,
}

impl ReplComponent for CollisionShape {}

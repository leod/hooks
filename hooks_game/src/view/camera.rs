use std::time::Duration;

use nalgebra::Point2;

pub struct Camera {
    pos: Point2<f32>,
}

impl Camera {
    pub fn new() -> Camera {
        Camera {
            pos: Point2::origin(),
        }
    }

    pub fn set_pos(&mut self, pos: Point2<f32>) {
        self.pos = pos;
    }

    pub fn update(&mut self, delta: Duration) {}
}

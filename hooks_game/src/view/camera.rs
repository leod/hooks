use std::time::Duration;

use nalgebra::{Matrix4, Point2, Translation3, Vector2, Vector3};

pub struct Camera {
    window_size: Vector2<f32>,
    pos: Point2<f32>,
}

impl Camera {
    pub fn new((window_width, window_height): (u32, u32)) -> Camera {
        Camera {
            window_size: Vector2::new(window_width as f32, window_height as f32),
            pos: Point2::origin(),
        }
    }

    pub fn set_pos(&mut self, pos: Point2<f32>) {
        self.pos = pos;
    }

    pub fn update(&mut self, delta: Duration) {}

    pub fn transform(&self) -> Matrix4<f32> {
        let coords = -self.pos.coords + self.window_size / 2.0;

        Translation3::from_vector(Vector3::new(coords.x, coords.y, 0.0)).to_homogeneous()
    }
}

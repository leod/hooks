use std::time::Duration;

use nalgebra::{norm, Matrix4, Point2, Similarity3, Translation3, UnitQuaternion, Vector2, Vector3};

use hooks_util::timer::duration_to_secs;

pub struct Camera {
    window_size: Vector2<f32>,
    target_pos: Point2<f32>,
    pos: Point2<f32>,
}

impl Camera {
    pub fn new((window_width, window_height): (u32, u32)) -> Camera {
        Camera {
            window_size: Vector2::new(window_width as f32, window_height as f32),
            target_pos: Point2::origin(),
            pos: Point2::origin(),
        }
    }

    pub fn set_target_pos(&mut self, target_pos: Point2<f32>) {
        self.target_pos = target_pos;
    }

    pub fn update(&mut self, delta_time: Duration) {
        let speed = 0.2;
        let delta_pos = self.target_pos - self.pos;
        let distance = norm(&delta_pos);
        let v = delta_pos * distance * speed;
        let t = (duration_to_secs(delta_time) as f32).min(speed / distance);

        self.pos += v * t;
    }

    pub fn similarity(&self) -> Similarity3<f32> {
        let scale = 0.75;
        let coords = -self.pos.coords * scale + self.window_size / 2.0;
        let trans = Translation3::from_vector(Vector3::new(coords.x, coords.y, 0.0));
        let rot = UnitQuaternion::identity();

        Similarity3::from_parts(trans, rot, scale)
    }

    pub fn transform(&self) -> Matrix4<f32> {
        self.similarity().to_homogeneous()
    }
}

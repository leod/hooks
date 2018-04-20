//! Bot player that just executes random input for now.

use rand::{self, Rng};

use hooks_game::PlayerInput;

#[derive(Default)]
pub struct Bot {
    input: PlayerInput,
}

const SHOOT_PROB: f64 = 0.01;
const PULL_PROB: f64 = 0.005;
const MOVE_PROB: f64 = 0.95;
const ROT_SPEED: f32 = 3.0;

impl Bot {
    /// Returns the player input for the next tick.
    pub fn run(&mut self) -> PlayerInput {
        let mut rng = rand::thread_rng();

        if rng.gen::<f64>() <= SHOOT_PROB {
            self.input.shoot_one = !self.input.shoot_one;
        }
        if rng.gen::<f64>() <= SHOOT_PROB {
            self.input.shoot_two = !self.input.shoot_two;
        }
        if rng.gen::<f64>() <= PULL_PROB {
            self.input.pull_one = !self.input.pull_one;
        }
        if rng.gen::<f64>() <= PULL_PROB {
            self.input.pull_two = !self.input.pull_two;
        }
        self.input.move_forward = rng.gen::<f64>() <= MOVE_PROB;
        if rng.gen::<bool>() {
            self.input.rot_angle += rng.gen::<f32>() * ROT_SPEED;
        } else {
            self.input.rot_angle -= rng.gen::<f32>() * ROT_SPEED;
        }

        self.input.clone()
    }
}

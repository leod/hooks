use nalgebra::{Isometry3, Matrix4, Vector3, Vector4};
use specs::prelude::{Join, ReadStorage, SystemData, World};

use ggez;
use ggez::graphics::{self, Drawable, DrawParam};

use hooks_game::game::entity::player::{Player, HEIGHT, WIDTH};
use hooks_game::physics::{Orientation, Position};
use hooks_util::profile;

use {with_transform, Input, Registry};

pub fn register_show(reg: &mut Registry) {
    reg.draw_fn(draw);
}

type DrawData<'a> = (
    ReadStorage<'a, Position>,
    ReadStorage<'a, Orientation>,
    ReadStorage<'a, Player>,
);

fn draw(ctx: &mut ggez::Context, input: &Input, world: &World) -> ggez::error::GameResult<()> {
    profile!("player");

    let (position, orientation, player) = DrawData::fetch(&world.res);

    for (position, orientation, _) in (&position, &orientation, &player).join() {
        let coords = position.0.coords;
        let scaling = Matrix4::from_diagonal(&Vector4::new(WIDTH / 2.0, HEIGHT / 6.0, 1.0, 1.0));
        let isometry = Isometry3::new(
            Vector3::new(coords.x, coords.y, 0.0),
            orientation.0 * Vector3::z_axis().unwrap(),
        );
        let matrix = isometry.to_homogeneous() * scaling;

        with_transform(ctx, matrix, |ctx| {
            input
                .assets
                .rect_towards_x_fill
                .draw(ctx, DrawParam::new().color([1.0, 1.0, 1.0, 1.0]))
        })?;
    }

    Ok(())
}

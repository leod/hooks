use nalgebra::{Isometry3, Matrix4, Point2, Vector3, Vector4};
use specs::prelude::{Join, ReadStorage, SystemData, World};

use ggez;
use ggez::graphics::{self, Drawable, DrawParam};

use hooks_game::game::entity::wall;
use hooks_game::physics::{Orientation, Position};
use hooks_util::profile;

use {with_transform, Input, Registry};

pub fn register_show(reg: &mut Registry) {
    reg.draw_fn(draw);
}

type DrawData<'a> = (
    ReadStorage<'a, Position>,
    ReadStorage<'a, Orientation>,
    ReadStorage<'a, wall::Size>,
);

fn draw(ctx: &mut ggez::Context, input: &Input, world: &World) -> ggez::error::GameResult<()> {
    profile!("wall");

    let (position, orientation, size) = DrawData::fetch(&world.res);

    for (position, orientation, size) in (&position, &orientation, &size).join() {
        let coords = position.0.coords;
        let isometry = Isometry3::new(
            Vector3::new(coords.x, coords.y, 0.0),
            orientation.0 * Vector3::z_axis().unwrap(),
        );
        let scaling = Matrix4::from_diagonal(&Vector4::new(size.0.x, size.0.y, 1.0, 1.0));
        let matrix = isometry.to_homogeneous() * scaling;

        with_transform(ctx, matrix, |ctx| {
            input.assets.rect_fill.draw(
                ctx,
                DrawParam::new().color([1.0, 1.0, 1.0, 1.0])
            )
        })?;

        let outline = 10.0;
        let scaling = Matrix4::from_diagonal(&Vector4::new(
            size.0.x - outline,
            size.0.y - outline,
            1.0,
            1.0,
        ));
        let matrix = isometry.to_homogeneous() * scaling;

        with_transform(ctx, matrix, |ctx| {
            input.assets.rect_fill.draw(
                ctx,
                DrawParam::new().color([0.4, 0.4, 0.4, 1.0]),
            )
        })?;
    }

    Ok(())
}

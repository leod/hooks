use nalgebra::{Isometry3, Matrix4, Point2, Vector3, Vector4};
use specs::prelude::{Join, ReadStorage, SystemData, World};

use ggez;
use ggez::graphics::{self, Drawable};

use hooks_util::profile;
use hooks_common::game::entity::wall;
use hooks_common::physics::{Orientation, Position};

use {Input, Registry};

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
        let scaling = Matrix4::from_diagonal(&Vector4::new(size.0.x, size.0.y, 1.0, 1.0));
        let isometry = Isometry3::new(
            Vector3::new(coords.x, coords.y, 0.0),
            orientation.0 * Vector3::z_axis().unwrap(),
        );
        let matrix = isometry.to_homogeneous() * scaling;

        let curr_transform = graphics::get_transform(ctx);
        graphics::push_transform(ctx, Some(curr_transform * matrix));
        graphics::apply_transformations(ctx)?;

        graphics::set_color(
            ctx,
            graphics::Color {
                r: 0.7,
                g: 0.7,
                b: 0.7,
                a: 1.0,
            },
        )?;
        input.assets.rect_fill.draw(ctx, Point2::origin(), 0.0)?;

        graphics::pop_transform(ctx);
    }

    graphics::set_color(ctx, graphics::WHITE)?;

    Ok(())
}

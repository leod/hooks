use nalgebra::{Isometry3, Matrix4, Point2, Vector3, Vector4};
use shred::SystemData;
use specs::{Join, ReadStorage, VecStorage, World};

use ggez;
use ggez::graphics::{self, Drawable};

use common;
use common::game::entity::wall;
use common::physics::{Orientation, Position};

use show::{self, Assets};

// TODO: Lots of code duplication with show::rect! Ideally, we would automatically attach
// `show::rect::Draw` for entities with `wall::Size` components, I think. This isn't easily
// possible right now because we have the ctor stuff only for replicated entities, which walls do
// not need to be.

pub fn register_show(reg: &mut show::Registry) {
    reg.draw_fn(draw);
}

type DrawData<'a> = (
    ReadStorage<'a, Position>,
    ReadStorage<'a, Orientation>,
    ReadStorage<'a, wall::Size>,
);

fn draw(ctx: &mut ggez::Context, assets: &Assets, world: &World) -> ggez::error::GameResult<()> {
    let (position, orientation, size) = DrawData::fetch(&world.res, 0);

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

        assets.rect_fill.draw(ctx, Point2::origin(), 0.0)?;

        graphics::pop_transform(ctx);
    }

    Ok(())
}

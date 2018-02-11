use nalgebra::{norm, Isometry3, Matrix4, Point2, Vector3, Vector4};
use specs::{Join, ReadStorage, SystemData, World};

use ggez;
use ggez::graphics::{self, Drawable};

use hooks_common::entity::Active;
use hooks_common::physics::{Joints, Position};

use {Assets, Registry};

/// Draw joints for debugging.
pub fn register_show(reg: &mut Registry) {
    reg.draw_fn(draw);
}

type DrawData<'a> = (
    ReadStorage<'a, Active>,
    ReadStorage<'a, Position>,
    ReadStorage<'a, Joints>,
);

fn draw(ctx: &mut ggez::Context, assets: &Assets, world: &World) -> ggez::error::GameResult<()> {
    let (active, position, joints) = DrawData::fetch(&world.res, 0);

    for (_, position_a, joints) in (&active, &position, &joints).join() {
        for &(entity_b, _) in &joints.0 {
            if active.get(entity_b).is_none() {
                continue;
            }

            let position_b = position.get(entity_b).unwrap();

            let center = (position_a.0.coords + position_b.0.coords) / 2.0;
            let delta = position_b.0.coords - position_a.0.coords;
            let size = norm(&delta);
            let angle = delta.y.atan2(delta.x);

            let scaling = Matrix4::from_diagonal(&Vector4::new(size, 2.0, 1.0, 1.0));
            let isometry = Isometry3::new(
                Vector3::new(center.x, center.y, 0.0),
                angle * Vector3::z_axis().unwrap(),
            );
            let matrix = isometry.to_homogeneous() * scaling;

            let curr_transform = graphics::get_transform(ctx);
            graphics::push_transform(ctx, Some(curr_transform * matrix));
            graphics::apply_transformations(ctx)?;

            let color = graphics::Color {
                r: 0.0,
                g: 1.0,
                b: 0.0,
                a: 1.0,
            };
            graphics::set_color(ctx, color)?;

            assets.rect_fill.draw(ctx, Point2::origin(), 0.0)?;

            graphics::pop_transform(ctx);
        }
    }

    graphics::set_color(ctx, graphics::WHITE)?;

    Ok(())
}

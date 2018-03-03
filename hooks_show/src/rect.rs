use nalgebra::{Isometry3, Matrix4, Point2, Vector3, Vector4};
use specs::{Join, ReadStorage, SystemData, VecStorage, World};

use ggez;
use ggez::graphics::{self, Drawable};

use hooks_util::profile;
use hooks_common;
use hooks_common::entity::Active;
use hooks_common::physics::{Orientation, Position};

use {Assets, Registry};

pub fn register(reg: &mut hooks_common::Registry) {
    reg.component::<Draw>();
}

pub fn register_show(reg: &mut Registry) {
    reg.draw_fn(draw);
}

#[derive(Component, Clone, Debug)]
#[component(VecStorage)]
pub struct Draw {
    pub width: f32,
    pub height: f32,
}

type DrawData<'a> = (
    ReadStorage<'a, Active>,
    ReadStorage<'a, Position>,
    ReadStorage<'a, Orientation>,
    ReadStorage<'a, Draw>,
);

fn draw(ctx: &mut ggez::Context, assets: &Assets, world: &World) -> ggez::error::GameResult<()> {
    profile!("rect");

    let (active, position, orientation, draw) = DrawData::fetch(&world.res, 0);

    for (_active, position, orientation, draw) in (&active, &position, &orientation, &draw).join() {
        let coords = position.0.coords;
        let scaling = Matrix4::from_diagonal(&Vector4::new(draw.width, draw.height, 1.0, 1.0));
        let isometry = Isometry3::new(
            Vector3::new(coords.x, coords.y, 0.0),
            orientation.0 * Vector3::z_axis().unwrap(),
        );
        let matrix = isometry.to_homogeneous() * scaling;

        let curr_transform = graphics::get_transform(ctx);
        graphics::push_transform(ctx, Some(curr_transform * matrix));
        graphics::apply_transformations(ctx)?;

        let color = graphics::Color {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        };
        graphics::set_color(ctx, color)?;

        assets.rect_line.draw(ctx, Point2::origin(), 0.0)?;

        graphics::pop_transform(ctx);
    }

    graphics::set_color(ctx, graphics::WHITE)?;

    Ok(())
}

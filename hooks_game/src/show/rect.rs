use nalgebra::{Isometry3, Matrix4, Point2, Vector3, Vector4};
use shred::SystemData;
use specs::{Join, ReadStorage, VecStorage, World};

use ggez;
use ggez::graphics::{self, Drawable};

use common;
use common::physics::{Orientation, Position};

use show::{self, Assets};

pub fn register(reg: &mut common::Registry) {
    reg.component::<Draw>();
}

pub fn register_show(reg: &mut show::Registry) {
    reg.draw_fn(draw);
}

#[derive(Component, Clone, Debug)]
#[component(VecStorage)]
pub struct Draw {
    pub width: f32,
    pub height: f32,
}

type DrawData<'a> = (
    ReadStorage<'a, Position>,
    ReadStorage<'a, Orientation>,
    ReadStorage<'a, Draw>,
);

fn draw(ctx: &mut ggez::Context, assets: &Assets, world: &World) -> ggez::error::GameResult<()> {
    let (position, orientation, draw) = DrawData::fetch(&world.res, 0);

    for (position, orientation, draw) in (&position, &orientation, &draw).join() {
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

        assets.rect_line.draw(ctx, Point2::origin(), 0.0)?;

        graphics::pop_transform(ctx);
    }

    Ok(())
}

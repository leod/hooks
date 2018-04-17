use nalgebra::{Isometry3, Matrix4, Point2, Vector3, Vector4};
use specs::prelude::{Join, ReadStorage, SystemData, VecStorage, World};

use ggez;
use ggez::graphics::{self, Drawable};

use hooks_util::profile;
use hooks_common;
use hooks_common::repl;
use hooks_common::entity::Active;
use hooks_common::physics::{Orientation, Position};

use {Input, Registry, with_transform};

pub fn register(reg: &mut hooks_common::Registry) {
    reg.component::<Draw>();
}

pub fn register_show(reg: &mut Registry) {
    reg.draw_fn(draw);
}

#[derive(Component, Clone, Debug)]
#[storage(VecStorage)]
pub struct Draw {
    pub width: f32,
    pub height: f32,
    pub fill: bool,
}

type DrawData<'a> = (
    ReadStorage<'a, repl::Id>,
    ReadStorage<'a, Active>,
    ReadStorage<'a, Position>,
    ReadStorage<'a, Orientation>,
    ReadStorage<'a, Draw>,
);

fn draw(ctx: &mut ggez::Context, input: &Input, world: &World) -> ggez::error::GameResult<()> {
    profile!("rect");

    let (repl_id, active, position, orientation, draw) = DrawData::fetch(&world.res);

    for (repl_id, _active, position, orientation, draw) in
        (&repl_id, &active, &position, &orientation, &draw).join()
    {
        //debug!("rect at {}", position.0);

        let coords = position.0.coords;
        let scaling = Matrix4::from_diagonal(&Vector4::new(draw.width, draw.height, 1.0, 1.0));
        let isometry = Isometry3::new(
            Vector3::new(coords.x, coords.y, 0.0),
            orientation.0 * Vector3::z_axis().unwrap(),
        );
        let matrix = isometry.to_homogeneous() * scaling;

        let color = if (repl_id.0).0 == input.my_player_id {
            graphics::Color {
                r: 0.0,
                g: 0.0,
                b: 1.0,
                a: 1.0,
            }
        } else {
            graphics::Color {
                r: 1.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }
        };
        graphics::set_color(ctx, color)?;

        with_transform(ctx, matrix, |ctx| {
            if draw.fill {
                input.assets.rect_fill.draw(ctx, Point2::origin(), 0.0)
            } else {
                input.assets.rect_line.draw(ctx, Point2::origin(), 0.0)
            }
        })?;
    }

    graphics::set_color(ctx, graphics::WHITE)?;

    Ok(())
}

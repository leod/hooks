use nalgebra::{norm, Isometry3, Matrix4, Point2, Vector3, Vector4};
use specs::{Fetch, Join, ReadStorage, SystemData, World};

use ggez;
use ggez::graphics::{self, Drawable};

use hooks_common::entity::Active;
use hooks_common::repl::{self, EntityMap};
use hooks_common::game::entity::player::{active_hook_segment_entities, Hook, HookSegment};
use hooks_common::physics::Position;

use {Assets, Registry};

/// Draw joints for debugging.
pub fn register_show(reg: &mut Registry) {
    reg.draw_fn(draw);
}

type DrawData<'a> = (
    Fetch<'a, EntityMap>,
    ReadStorage<'a, repl::Id>,
    ReadStorage<'a, Active>,
    ReadStorage<'a, Position>,
    ReadStorage<'a, Hook>,
    ReadStorage<'a, HookSegment>,
);

fn draw(ctx: &mut ggez::Context, assets: &Assets, world: &World) -> ggez::error::GameResult<()> {
    let (entity_map, repl_id, active, position, hook, segment) = DrawData::fetch(&world.res, 0);

    for (is_active, &repl::Id((owner, _)), pos, hook) in
        (&active, &repl_id, &position, &hook).join()
    {
        if !is_active.0 {
            continue;
        }

        let first_segment_id = (owner, hook.first_segment_index);

        let segments =
            active_hook_segment_entities(&entity_map, &active, &segment, first_segment_id).unwrap();

        let mut prev_pos = pos.0;

        for &new_segment in segments.iter() {
            let new_pos = position.get(new_segment).unwrap().0;

            let center = (new_pos.coords + prev_pos.coords) / 2.0;
            let delta = new_pos.coords - prev_pos.coords;
            prev_pos = new_pos;

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

use nalgebra::{Isometry3, Matrix4, Point2, Rotation2, Vector3, Vector4};
use specs::{Fetch, Join, ReadStorage, SystemData, World};

use ggez;
use ggez::graphics::{self, Drawable};

use hooks_common::entity::Active;
use hooks_common::physics::{Orientation, Position};
use hooks_common::repl::{self, EntityMap};
use hooks_common::game::entity::player::{active_hook_segment_entities, Hook, HookSegment,
                                         HOOK_SEGMENT_LENGTH};

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
    ReadStorage<'a, Orientation>,
    ReadStorage<'a, Hook>,
    ReadStorage<'a, HookSegment>,
);

fn draw(ctx: &mut ggez::Context, assets: &Assets, world: &World) -> ggez::error::GameResult<()> {
    let (entity_map, repl_id, active, position, orientation, hook, hook_segment) =
        DrawData::fetch(&world.res, 0);

    for (is_active, &repl::Id((owner, _)), hook) in (&active, &repl_id, &hook).join() {
        if !is_active.0 {
            continue;
        }

        let first_segment_id = (owner, hook.first_segment_index);

        let segments =
            active_hook_segment_entities(&entity_map, &active, &hook_segment, first_segment_id)
                .unwrap();

        // Draw segment rects
        for &segment in segments.iter() {
            if !active.get(segment).unwrap().0 {
                continue;
            }

            // TODO: specs unwrap
            let pos = position.get(segment).unwrap().0.coords;
            let angle = orientation.get(segment).unwrap().0;

            let scaling = Matrix4::from_diagonal(&Vector4::new(HOOK_SEGMENT_LENGTH, 6.0, 1.0, 1.0));
            let isometry = Isometry3::new(
                Vector3::new(pos.x, pos.y, 0.0),
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

        // Draw end point squares
        for &segment in segments.iter() {
            if !active.get(segment).unwrap().0 {
                continue;
            }

            let segment_data = hook_segment.get(segment).unwrap();

            // TODO: specs unwrap
            let pos = position.get(segment).unwrap().0.coords;
            let angle = orientation.get(segment).unwrap().0;

            let rot = Rotation2::new(angle).matrix().clone();
            let attach_p = rot * Point2::new(HOOK_SEGMENT_LENGTH / 2.0, 0.0) + pos;
            let size = if segment_data.is_last { 12.0 } else { 8.0 };
            let scaling = Matrix4::from_diagonal(&Vector4::new(size, size, 1.0, 1.0));
            let isometry = Isometry3::new(
                Vector3::new(attach_p.x, attach_p.y, 0.0),
                angle * Vector3::z_axis().unwrap(),
            );
            let matrix = isometry.to_homogeneous() * scaling;

            let curr_transform = graphics::get_transform(ctx);
            graphics::push_transform(ctx, Some(curr_transform * matrix));
            graphics::apply_transformations(ctx)?;
            let color = if segment_data.fixed.is_some() {
                graphics::Color {
                    r: 1.0,
                    g: 0.0,
                    b: 0.0,
                    a: 1.0,
                }
            } else if segment_data.is_last {
                graphics::Color {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 1.0,
                }
            } else {
                graphics::Color {
                    r: 0.0,
                    g: 0.0,
                    b: 1.0,
                    a: 1.0,
                }
            };
            graphics::set_color(ctx, color)?;
            assets.rect_fill.draw(ctx, Point2::origin(), 0.0)?;
            graphics::pop_transform(ctx);
        }
    }

    graphics::set_color(ctx, graphics::WHITE)?;

    Ok(())
}

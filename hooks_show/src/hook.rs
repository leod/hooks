use std::f32;

use nalgebra::{Isometry3, Matrix4, Point2, Rotation2, Vector3, Vector4};
use specs::prelude::{Fetch, Join, ReadStorage, SystemData, World};

use ggez;
use ggez::graphics::{self, Drawable};

use particle_frenzy;

use hooks_util::profile;
use hooks_common::event::Event;
use hooks_common::physics::{Orientation, Position};
use hooks_common::repl::EntityMap;
use hooks_common::game::entity::hook;

use {Input, Output, Registry};

/// Draw joints for debugging.
pub fn register_show(reg: &mut Registry) {
    //reg.event_handler(handle_event);
    reg.draw_fn(draw);
}

fn handle_event(
    input: &Input,
    output: &mut Output,
    _: &mut World,
    events: &[Box<Event>],
) -> ggez::error::GameResult<()> {
    for event in events {
        match_event!(event:
            hook::FixedEvent => {
                let cone = particle_frenzy::spawn::Cone {
                    spawn_time: input.time,
                    life_time: 0.3,
                    pos: event.pos,
                    orientation: event.vel[1].atan2(event.vel[0]),
                    spread: f32::consts::PI * 2.0, //f32::consts::PI / 12.0,
                    min_speed: 1.0,
                    max_speed: 1500.0,
                    angle: 0.0,
                    friction: 4000.0,
                    size: [3.0, 3.0],
                    color: |_, speed| {
                        if event.hook_index == 0 {
                            [0.0, 0.5, (speed / 1000.0)]
                        } else {
                            [(speed / 1000.0), 0.5, 0.0]
                        }
                    }
                };
                cone.spawn(&mut output.particles, 10000);
            },
        );
    }

    Ok(())
}

type DrawData<'a> = (
    Fetch<'a, EntityMap>,
    ReadStorage<'a, Position>,
    ReadStorage<'a, Orientation>,
    ReadStorage<'a, hook::Def>,
    ReadStorage<'a, hook::State>,
);

fn draw(ctx: &mut ggez::Context, input: &Input, world: &World) -> ggez::error::GameResult<()> {
    profile!("hook");

    let (entity_map, position, orientation, hook_def, hook_state) = DrawData::fetch(&world.res);

    for (hook_def, hook_state) in (&hook_def, &hook_state).join() {
        if let &Some(hook::ActiveState {
            num_active_segments,
            ref fixed,
            ..
        }) = &hook_state.0
        {
            // Look up our segments
            let mut segments = Vec::new();
            for i in 0..num_active_segments as usize {
                // TODO: repl unwrap
                // TODO: num_active_segments could be out of bounds
                segments.push(entity_map.try_id_to_entity(hook_def.segments[i]).unwrap());
            }

            // Draw segment rects
            for &segment in segments.iter() {
                // TODO: specs unwrap
                let pos = position.get(segment).unwrap().0.coords;
                let angle = orientation.get(segment).unwrap().0;

                let scaling =
                    Matrix4::from_diagonal(&Vector4::new(hook::SEGMENT_LENGTH, 3.0, 1.0, 1.0));
                let isometry = Isometry3::new(
                    Vector3::new(pos.x, pos.y, 0.0),
                    angle * Vector3::z_axis().unwrap(),
                );
                let matrix = isometry.to_homogeneous() * scaling;

                let curr_transform = graphics::get_transform(ctx);
                graphics::push_transform(ctx, Some(curr_transform * matrix));
                graphics::apply_transformations(ctx)?;
                let color = if hook_def.index == 0 {
                    graphics::Color {
                        r: 0.0,
                        g: 1.0,
                        b: 0.0,
                        a: 1.0,
                    }
                } else {
                    graphics::Color {
                        r: 1.0,
                        g: 0.5,
                        b: 0.0,
                        a: 1.0,
                    }
                };
                graphics::set_color(ctx, color)?;
                input.assets.rect_fill.draw(ctx, Point2::origin(), 0.0)?;
                graphics::pop_transform(ctx);
            }

            // Draw end point squares
            for (i, &segment) in segments.iter().enumerate() {
                // TODO: specs unwrap
                let pos = position.get(segment).unwrap().0.coords;
                let angle = orientation.get(segment).unwrap().0;

                let rot = Rotation2::new(angle).matrix().clone();
                let attach_p = rot * Point2::new(hook::SEGMENT_LENGTH / 2.0, 0.0) + pos;
                let size = if i == 0 { 12.0 } else { 4.0 };
                let scaling = Matrix4::from_diagonal(&Vector4::new(size, size, 1.0, 1.0));
                let isometry = Isometry3::new(
                    Vector3::new(attach_p.x, attach_p.y, 0.0),
                    angle * Vector3::z_axis().unwrap(),
                );
                let matrix = isometry.to_homogeneous() * scaling;

                let curr_transform = graphics::get_transform(ctx);
                graphics::push_transform(ctx, Some(curr_transform * matrix));
                graphics::apply_transformations(ctx)?;
                let color = if i == 0 && fixed.is_some() {
                    graphics::Color {
                        r: 1.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    }
                } else if i == 0 {
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
                input.assets.rect_fill.draw(ctx, Point2::origin(), 0.0)?;
                graphics::pop_transform(ctx);
            }
        }
    }

    graphics::set_color(ctx, graphics::WHITE)?;

    Ok(())
}

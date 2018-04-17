//! This crate is a bit of a placeholder to have some simple graphics.

extern crate gfx_device_gl;
extern crate ggez;
#[macro_use]
extern crate hooks_common;
#[macro_use]
extern crate hooks_util;
#[macro_use]
extern crate log;
extern crate nalgebra;
extern crate particle_frenzy;
extern crate specs;
#[macro_use]
extern crate specs_derive;

mod camera;
mod rect;
mod wall;
mod hook;
mod player;
mod entity;
pub mod debug;

use nalgebra::{Point2, Matrix4};

use specs::prelude::{Entity, World};

use ggez::graphics::{self, DrawMode, Mesh};

use hooks_util::profile;
use hooks_common::{Event, GameInfo, PlayerId};
use hooks_common::physics::Position;
use hooks_common::repl::player::Players;

use self::camera::Camera;

pub fn register(reg: &mut hooks_common::Registry) {
    rect::register(reg);
    entity::register(reg);
}

pub fn register_show(reg: &mut Registry) {
    wall::register_show(reg);
    hook::register_show(reg);
    player::register_show(reg);
    rect::register_show(reg);
}

pub struct Assets {
    pub rect_fill: Mesh,
    pub rect_line: Mesh,
    pub rect_towards_x_fill: Mesh,
}

pub struct Input {
    pub my_player_id: PlayerId,
    pub assets: Assets,
    pub time: f32,
}

pub struct Output {
    pub particles: particle_frenzy::System<gfx_device_gl::Resources>,
}

impl Assets {
    pub fn new(ctx: &mut ggez::Context) -> ggez::error::GameResult<Assets> {
        let rect_fill = Mesh::new_polygon(
            ctx,
            DrawMode::Fill,
            &[
                Point2::new(-0.5, -0.5),
                Point2::new(0.5, -0.5),
                Point2::new(0.5, 0.5),
                Point2::new(-0.5, 0.5),
            ],
        )?;
        let rect_line = Mesh::new_polygon(
            ctx,
            DrawMode::Line(0.1),
            &[
                Point2::new(-0.5, -0.5),
                Point2::new(0.5, -0.5),
                Point2::new(0.5, 0.5),
                Point2::new(-0.5, 0.5),
            ],
        )?;
        let rect_towards_x_fill = Mesh::new_polygon(
            ctx,
            DrawMode::Fill,
            &[
                Point2::new(0.0, -0.5),
                Point2::new(1.0, -0.5),
                Point2::new(1.0, 0.5),
                Point2::new(0.0, 0.5),
            ],
        )?;

        Ok(Assets {
            rect_fill,
            rect_line,
            rect_towards_x_fill,
        })
    }
}

pub type EventHandler =
    fn(&Input, &mut Output, &mut World, &[Box<Event>]) -> ggez::error::GameResult<()>;
pub type DrawFn = fn(&mut ggez::Context, &Input, &World) -> ggez::error::GameResult<()>;

#[derive(Default)]
pub struct Registry {
    event_handlers: Vec<EventHandler>,
    draw_fns: Vec<DrawFn>,
}

impl Registry {
    /// Register a new game event handler. Only called at the start of a game.
    pub fn event_handler(&mut self, f: EventHandler) {
        self.event_handlers.push(f);
    }

    /// Register a new drawing function. Only called at the start of a game.
    pub fn draw_fn(&mut self, f: DrawFn) {
        self.draw_fns.push(f);
    }
}

pub fn with_transform<F, T>(
    ctx: &mut ggez::Context,
    transform: Matrix4<f32>,
    f: F
) -> ggez::error::GameResult<T>
where
    F: FnOnce(&mut ggez::Context) -> ggez::error::GameResult<T>
{
    let current_transform = graphics::get_transform(ctx);
    graphics::push_transform(ctx, Some(current_transform * transform));
    graphics::apply_transformations(ctx)?;

    let result = f(ctx);

    graphics::pop_transform(ctx);

    result
}

pub struct Show {
    my_player_id: PlayerId,
    game_info: GameInfo,
    reg: Registry,

    camera: Camera,

    input: Input,
    output: Output,
}

impl Show {
    /// Load all assets for a game info and create a `Show`.
    pub fn load(
        ctx: &mut ggez::Context,
        view_size: (u32, u32),
        my_player_id: PlayerId,
        game_info: &GameInfo,
        assets: Assets,
    ) -> ggez::error::GameResult<Show> {
        let mut reg = Registry::default();
        register_show(&mut reg);

        let particles = {
            let target = graphics::get_screen_render_target(ctx);
            let factory = graphics::get_factory(ctx);
            particle_frenzy::System::new(factory, target, 10_000, 50)
        };

        let input = Input {
            my_player_id,
            assets,
            time: 0.0,
        };

        let output = Output { particles };

        Ok(Show {
            my_player_id,
            game_info: game_info.clone(),
            reg,
            camera: Camera::new(view_size),
            input,
            output,
        })
    }

    /// Show game events.
    pub fn handle_events(
        &mut self,
        ctx: &mut ggez::Context,
        world: &mut World,
        events: &[Box<Event>],
    ) -> ggez::error::GameResult<()> {
        profile!("show events");

        self.update_time(ctx);

        for handler in &self.reg.event_handlers {
            handler(&self.input, &mut self.output, world, events)?;
        }

        Ok(())
    }

    /// Draw the game.
    pub fn draw(&mut self, ctx: &mut ggez::Context, world: &World) -> ggez::error::GameResult<()> {
        profile!("show");

        self.update_time(ctx);
        let delta = ggez::timer::get_delta(ctx);

        let positions = world.read::<Position>();

        if let Some(my_entity) = self.my_player_entity(world) {
            if let Some(position) = positions.get(my_entity) {
                //debug!("camera at {}", position.0);
                self.camera.set_target_pos(position.0);
            }
        }

        self.camera.update(delta);

        let camera_transform = self.camera.transform();

        graphics::push_transform(ctx, Some(camera_transform));
        graphics::apply_transformations(ctx)?;

        {
            profile!("fns");

            for draw_fn in &self.reg.draw_fns {
                draw_fn(ctx, &self.input, world)?;
            }
        }

        graphics::pop_transform(ctx);
        graphics::apply_transformations(ctx)?;

        // Draw particles
        {
            profile!("particles");

            let transform = graphics::get_projection(ctx) * camera_transform;
            let (factory, device, encoder, _depthview, _colorview) = graphics::get_gfx_objects(ctx);
            self.output
                .particles
                .render(factory, encoder, self.input.time, &transform.into());
            encoder.flush(device);
        }

        Ok(())
    }

    /// Once the game is finished, move the `Assets` so that we don't reload things unnecessarily.
    pub fn into_assets(self) -> Assets {
        self.input.assets
    }

    pub fn camera(&self) -> &Camera {
        &self.camera
    }

    pub fn my_player_entity(&self, world: &World) -> Option<Entity> {
        let players = world.read_resource::<Players>();

        if let Some(player) = players.get(self.my_player_id) {
            player.entity
        } else {
            // We are connected, but have not received the first tick yet...
            // ... or the server isn't doing its job.
            None
        }
    }

    fn update_time(&mut self, ctx: &mut ggez::Context) {
        // TODO: Should this involve the game time somehow?

        self.input.time =
            ggez::timer::duration_to_f64(ggez::timer::get_time_since_start(ctx)) as f32;
    }
}

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
mod entity;
pub mod debug;

use nalgebra::Point2;

use specs::World;

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
    rect::register_show(reg);
}

pub struct Assets {
    pub rect_fill: Mesh,
    pub rect_line: Mesh,
}

pub struct Context {
    pub assets: Assets,
    pub particles: particle_frenzy::System<gfx_device_gl::Resources>,
    pub time: f32,
}

impl Assets {
    pub fn new(ctx: &mut ggez::Context) -> ggez::error::GameResult<Assets> {
        // TODO: Better place to put this
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
            DrawMode::Line(1.0),
            &[
                Point2::new(-0.5, -0.5),
                Point2::new(0.5, -0.5),
                Point2::new(0.5, 0.5),
                Point2::new(-0.5, 0.5),
            ],
        )?;

        Ok(Assets {
            rect_fill,
            rect_line,
        })
    }
}

pub type EventHandler = fn(&mut Context, &mut World, &[Box<Event>]) -> ggez::error::GameResult<()>;
pub type DrawFn = fn(&mut ggez::Context, &Assets, &World) -> ggez::error::GameResult<()>;

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

pub struct Show {
    my_player_id: PlayerId,
    game_info: GameInfo,
    reg: Registry,

    context: Context,
    camera: Camera,
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
        let context = Context {
            assets,
            particles,
            time: 0.0,
        };

        Ok(Show {
            my_player_id,
            game_info: game_info.clone(),
            reg,

            context,
            camera: Camera::new(view_size),
        })
    }

    /// Show game events.
    pub fn handle_events(
        &mut self,
        ctx: &mut ggez::Context,
        world: &mut World,
        events: &Vec<Box<Event>>,
    ) -> ggez::error::GameResult<()> {
        self.update_time(ctx);

        for handler in &self.reg.event_handlers {
            handler(&mut self.context, world, events)?;
        }

        Ok(())
    }

    /// Draw the game.
    pub fn draw(&mut self, ctx: &mut ggez::Context, world: &World) -> ggez::error::GameResult<()> {
        profile!("show game");

        self.update_time(ctx);
        let delta = ggez::timer::get_delta(ctx);

        let positions = world.read::<Position>();

        if let Some(my_entity) = self.my_player_entity(world) {
            self.camera
                .set_target_pos(positions.get(my_entity).unwrap().0);
        }

        self.camera.update(delta);

        let camera_transform = self.camera.transform();

        graphics::push_transform(ctx, Some(camera_transform));
        graphics::apply_transformations(ctx)?;

        for draw_fn in &self.reg.draw_fns {
            draw_fn(ctx, &self.context.assets, world)?;
        }

        graphics::pop_transform(ctx);
        graphics::apply_transformations(ctx)?;

        // Draw particles
        {
            let transform = graphics::get_projection(ctx) * camera_transform;
            let (factory, device, encoder, _depthview, _colorview) = graphics::get_gfx_objects(ctx);
            self.context
                .particles
                .render(factory, encoder, self.context.time, &transform.into());
            encoder.flush(device);
        }

        Ok(())
    }

    /// Once the game is finished, move the `Assets` so that we don't reload things unnecessarily.
    pub fn into_assets(self) -> Assets {
        self.context.assets
    }

    fn my_player_entity(&self, world: &World) -> Option<specs::Entity> {
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

        self.context.time =
            ggez::timer::duration_to_f64(ggez::timer::get_time_since_start(ctx)) as f32;
    }
}

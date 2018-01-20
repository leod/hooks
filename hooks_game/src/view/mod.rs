mod camera;
mod rect;
mod entity_types;

use nalgebra::Point2;

use specs::World;

use ggez;
use ggez::graphics::{self, DrawMode, Mesh};

use self::camera::Camera;
use common::{self, Event, GameInfo, PlayerId};

pub fn register(game_info: &GameInfo, reg: &mut common::Registry) {
    rect::register(reg);
    entity_types::register(reg);
}

pub fn register_load(game_info: &GameInfo, reg: &mut Registry) {
    rect::register_load(reg);
}

pub struct Assets {
    pub rect: Mesh,
}

impl Assets {
    pub fn new(ctx: &mut ggez::Context) -> ggez::error::GameResult<Assets> {
        // TODO: Better place to put this
        let rect = Mesh::new_polygon(
            ctx,
            DrawMode::Fill,
            &[
                Point2::new(-0.5, -0.5),
                Point2::new(0.5, -0.5),
                Point2::new(0.5, 0.5),
                Point2::new(-0.5, 0.5),
            ],
        )?;

        Ok(Assets { rect })
    }
}

pub type EventHandler = fn(&Assets, &mut World, &Vec<Box<Event>>) -> ggez::error::GameResult<()>;
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

pub struct View {
    my_player_id: PlayerId,
    game_info: GameInfo,

    assets: Assets,
    reg: Registry,

    camera: Camera,
}

impl View {
    /// Load all assets for a game info and create a `View`.
    pub fn load(
        view_size: (u32, u32),
        my_player_id: PlayerId,
        game_info: &GameInfo,
        assets: Assets,
    ) -> ggez::error::GameResult<View> {
        let mut reg = Registry::default();
        register_load(game_info, &mut reg);

        Ok(View {
            my_player_id,
            game_info: game_info.clone(),

            assets,
            reg,

            camera: Camera::new(view_size),
        })
    }

    /// View game events.
    pub fn handle_events(
        &self,
        world: &mut World,
        events: &Vec<Box<Event>>,
    ) -> ggez::error::GameResult<()> {
        for handler in &self.reg.event_handlers {
            handler(&self.assets, world, events)?;
        }

        Ok(())
    }

    /// Draw the game.
    pub fn draw(&mut self, ctx: &mut ggez::Context, world: &World) -> ggez::error::GameResult<()> {
        let delta = ggez::timer::get_delta(ctx);

        self.camera.update(delta);
        graphics::push_transform(ctx, Some(self.camera.transform()));
        graphics::apply_transformations(ctx)?;

        for draw_fn in &self.reg.draw_fns {
            draw_fn(ctx, &self.assets, world)?;
        }

        graphics::pop_transform(ctx);

        Ok(())
    }

    /// Once the game is finished, move the `Assets` so that we don't reload things unnecessarily.
    pub fn into_assets(self) -> Assets {
        self.assets
    }
}

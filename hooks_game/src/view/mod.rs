mod camera;

use specs::World;

use ggez;

use common::{self, Event, GameInfo, PlayerId};

use self::camera::Camera;

pub fn register(game_info: &GameInfo, reg: &mut common::Registry) {}

pub fn register_load(game_info: &GameInfo, view: &mut Registry) {}

#[derive(Default)]
pub struct Assets {}

pub type EventHandler = fn(&Assets, &mut World, &Vec<Box<Event>>) -> ggez::error::GameResult<()>;
pub type DrawFn = fn(&Assets, &mut ggez::Context, &mut World) -> ggez::error::GameResult<()>;

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

            camera: Camera::new(),
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
    pub fn draw(
        &mut self,
        ctx: &mut ggez::Context,
        world: &mut World,
    ) -> ggez::error::GameResult<()> {
        let delta = ggez::timer::get_delta(ctx);

        self.camera.update(delta);

        for draw_fn in &self.reg.draw_fns {
            draw_fn(&self.assets, ctx, world)?;
        }

        Ok(())
    }

    /// Once the game is finished, move the `Assets` so that we don't reload things unnecessarily.
    pub fn into_assets(self) -> Assets {
        self.assets
    }
}

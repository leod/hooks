use specs::World;

use ggez;

use common::{self, Event, GameInfo, Registry};

pub fn register(game_info: &GameInfo, reg: &mut Registry) {}

pub fn register_load(game_info: &GameInfo, view: &mut View) {}

#[derive(Default)]
pub struct Assets {}

pub type EventHandler = fn(&Assets, &mut World, &Vec<Box<Event>>) -> ggez::error::GameResult<()>;
pub type DrawFn = fn(&Assets, &mut ggez::Context, &mut World) -> ggez::error::GameResult<()>;

pub struct View {
    game_info: GameInfo,
    assets: Assets,
    event_handlers_post_tick: Vec<EventHandler>,
    draw_fns: Vec<DrawFn>,
}

impl View {
    /// Load all assets for a game info and create a `View`.
    pub fn load(game_info: &GameInfo, assets: Assets) -> View {
        let mut view = View {
            game_info: game_info.clone(),
            assets: assets,
            event_handlers_post_tick: Vec::new(),
            draw_fns: Vec::new(),
        };

        register_load(game_info, &mut view);

        view
    }

    /// Register a new game event handler. Only called at the start of a game.
    pub fn event_handler_post_tick(&mut self, f: EventHandler) {
        self.event_handlers_post_tick.push(f);
    }

    /// Register a new drawing function. Only called at the start of a game.
    pub fn draw_fn(&mut self, f: DrawFn) {
        self.draw_fns.push(f);
    }

    /// View game events.
    pub fn handle_events(
        &self,
        world: &mut World,
        events: &Vec<Box<Event>>,
    ) -> ggez::error::GameResult<()> {
        for handler in &self.event_handlers_post_tick {
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
        for draw_fn in &self.draw_fns {
            draw_fn(&self.assets, ctx, world)?;
        }

        Ok(())
    }

    /// Once the game is finished, move the `Assets` so that we don't reload things unnecessarily.
    pub fn into_assets(self) -> Assets {
        self.assets
    }
}

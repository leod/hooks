extern crate env_logger;
extern crate ggez;
extern crate hooks_game;
#[macro_use]
extern crate log;

use std::{env, path, thread};

use hooks_game::client::Client;
use hooks_game::game::{self, Game};

struct Config {
    host: String,
    port: u16,
    name: String,
}

struct MainState {
    client: Client,
    game: Game,
}

impl ggez::event::EventHandler for MainState {
    fn update(&mut self, ctx: &mut ggez::Context) -> ggez::error::GameResult<()> {
        let delta = ggez::timer::get_delta(ctx);

        match self.game.update(&mut self.client, delta).unwrap() {
            Some(game::Event::Disconnected) => {
                info!("Got disconnected! Bye.");
                ctx.quit()?;
            }
            Some(game::Event::TickStarted(events)) => {}
            None => {}
        }

        Ok(())
    }

    fn draw(&mut self, ctx: &mut ggez::Context) -> ggez::error::GameResult<()> {
        ggez::graphics::clear(ctx);

        ggez::graphics::present(ctx);

        Ok(())
    }

    fn mouse_button_down_event(
        &mut self,
        _ctx: &mut ggez::Context,
        _button: ggez::event::MouseButton,
        _x: i32,
        _y: i32,
    ) {
    }

    fn mouse_button_up_event(
        &mut self,
        _ctx: &mut ggez::Context,
        _button: ggez::event::MouseButton,
        _x: i32,
        _y: i32,
    ) {
    }

    fn mouse_motion_event(
        &mut self,
        _ctx: &mut ggez::Context,
        _state: ggez::event::MouseState,
        _x: i32,
        _y: i32,
        _xrel: i32,
        _yrel: i32,
    ) {
    }

    fn key_down_event(
        &mut self,
        _ctx: &mut ggez::Context,
        _keycode: ggez::event::Keycode,
        _keymod: ggez::event::Mod,
        _repeat: bool,
    ) {
    }

    fn key_up_event(
        &mut self,
        _ctx: &mut ggez::Context,
        _keycode: ggez::event::Keycode,
        _keymod: ggez::event::Mod,
        _repeat: bool,
    ) {
    }
}

fn main() {
    env_logger::init();

    let config = Config {
        host: "localhost".to_string(),
        port: 32444,
        name: "testy".to_string(),
    };
    let timeout_ms = 5000;

    // Connect to server
    let mut client = Client::connect(&config.host, config.port, &config.name, timeout_ms).unwrap();
    info!(
        "Connected to {}:{} with player id {} and game info {:?}",
        config.host,
        config.port,
        client.my_player_id(),
        client.game_info()
    );

    let mut game = Game::new(client.my_player_id(), client.game_info());
    client.ready().unwrap();

    // Initialize ggez
    let ctx = &mut ggez::Context::load_from_conf("hooks", "leod", ggez::conf::Conf::new()).unwrap();

    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        let mut path = path::PathBuf::from(manifest_dir);
        path.push("resources");
        ctx.filesystem.mount(&path, true);

        info!("Loading resources from {:?}", path);
    }

    let mut state = MainState { client, game };

    ggez::event::run(ctx, &mut state).unwrap();
}

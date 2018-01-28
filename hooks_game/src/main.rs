extern crate env_logger;
extern crate ggez;
extern crate hooks_common as common;
extern crate hooks_game as hooks_game;
#[macro_use]
extern crate log;
extern crate nalgebra;
extern crate specs;

use std::{env, path};

use ggez::event::Keycode;
use ggez::graphics::{self, Font, Text};
use nalgebra::{Point2, Vector2};

use common::defs::{GameInfo, PlayerInput};
use common::registry::Registry;

use hooks_game::client::Client;
use hooks_game::game::{self, Game};
use hooks_game::show::{self, Assets, Show};

fn register(reg: &mut Registry, game_info: &GameInfo) {
    // Game state
    game::register(reg, game_info);

    // Components for showing game state
    show::register(reg);
}

struct Config {
    host: String,
    port: u16,
    name: String,
}

struct MainState {
    client: Client,
    game: Game,

    show: Show,
    font: Font,
    show_debug: bool,

    next_player_input: PlayerInput,
}

fn debug_text(
    ctx: &mut ggez::Context,
    text: &str,
    pos: Point2<f32>,
    font: &Font,
) -> ggez::error::GameResult<()> {
    let lines_columns: Vec<Vec<&str>> = text.lines()
        .map(|line| line.split('\t').collect())
        .collect();

    /*let column_lens = lines_columns
        .iter()
        .scan(*/

    Ok(())
}

impl ggez::event::EventHandler for MainState {
    fn update(&mut self, ctx: &mut ggez::Context) -> ggez::error::GameResult<()> {
        let delta = ggez::timer::get_delta(ctx);

        match self.game
            .update(&mut self.client, &self.next_player_input, delta)
            .unwrap()
        {
            Some(game::Event::Disconnected) => {
                info!("Got disconnected! Bye.");
                ctx.quit()?;
            }
            Some(game::Event::TickStarted(ref events)) => {
                self.show.handle_events(self.game.world_mut(), events)?;
            }
            None => {}
        }

        Ok(())
    }

    fn draw(&mut self, ctx: &mut ggez::Context) -> ggez::error::GameResult<()> {
        ggez::graphics::clear(ctx);

        self.show.draw(ctx, self.game.world())?;

        if self.show_debug {
            let text = Text::new(ctx, &format!("{:?}", self.game), &self.font).unwrap();
            graphics::draw(ctx, &text, Point2::new(10.0, 10.0), 0.0).unwrap();

            let text = Text::new(ctx, "Hello world!", &self.font).unwrap();
            graphics::draw(ctx, &text, Point2::new(10.0, 10.0), 0.0).unwrap();
        }

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
        ctx: &mut ggez::Context,
        _state: ggez::event::MouseState,
        x: i32,
        y: i32,
        _xrel: i32,
        _yrel: i32,
    ) {
        let (size_x, size_y) = ggez::graphics::get_size(ctx);
        let size = Vector2::new(size_x as f32, size_y as f32);
        let clip = Vector2::new(
            x.max(0).min(size_x as i32) as f32,
            y.max(0).min(size_y as i32) as f32,
        );
        let shift = clip - size / 2.0;

        self.next_player_input.rot_angle = shift.y.atan2(shift.x)
    }

    fn key_down_event(
        &mut self,
        _ctx: &mut ggez::Context,
        keycode: Keycode,
        _keymod: ggez::event::Mod,
        _repeat: bool,
    ) {
        match keycode {
            Keycode::W => self.next_player_input.move_forward = true,
            Keycode::S => self.next_player_input.move_backward = true,
            _ => {}
        }
    }

    fn key_up_event(
        &mut self,
        _ctx: &mut ggez::Context,
        keycode: ggez::event::Keycode,
        _keymod: ggez::event::Mod,
        _repeat: bool,
    ) {
        match keycode {
            Keycode::W => self.next_player_input.move_forward = false,
            Keycode::S => self.next_player_input.move_backward = false,
            _ => {}
        }
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

    // Register and create game
    let mut reg = Registry::new();
    register(&mut reg, client.game_info());

    let game = Game::new(reg, client.my_player_id(), client.game_info());

    // Initialize ggez
    let ctx = &mut ggez::Context::load_from_conf("hooks", "leod", ggez::conf::Conf::new()).unwrap();

    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        let mut path = path::PathBuf::from(manifest_dir);
        path.push("resources");
        ctx.filesystem.mount(&path, true);

        info!("Loading resources from {:?}", path);
    }

    let assets = Assets::new(ctx).unwrap();
    let show = Show::load(
        ggez::graphics::get_size(ctx),
        client.my_player_id(),
        client.game_info(),
        assets,
    ).unwrap();
    let font = Font::default_font().unwrap();

    // Inform the server that we are good to go
    client.ready().unwrap();

    let mut state = MainState {
        client,
        game,
        show,
        font,
        show_debug: true,
        next_player_input: PlayerInput::default(),
    };
    ggez::event::run(ctx, &mut state).unwrap();
}

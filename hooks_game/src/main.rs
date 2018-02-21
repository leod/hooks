extern crate env_logger;
extern crate ggez;
extern crate hooks_common;
extern crate hooks_game;
extern crate hooks_show;
#[macro_use]
extern crate hooks_util;
#[macro_use]
extern crate log;
extern crate nalgebra;
extern crate specs;

use std::{env, path, thread};

use nalgebra::{Point2, Vector2};

use ggez::event::{self, Keycode, MouseButton};
use ggez::graphics::Font;

use hooks_util::debug::{self, Inspect};
use hooks_util::profile::{self, PROFILER};
use hooks_common::defs::{GameInfo, PlayerInput};
use hooks_common::registry::Registry;
use hooks_game::client::Client;
use hooks_game::game::Game;
use hooks_show::{Assets, Show};

fn register(reg: &mut Registry, game_info: &GameInfo) {
    // Game state
    hooks_game::game::register(reg, game_info);

    // Components for showing game state
    hooks_show::register(reg);
}

struct Config {
    host: String,
    port: u16,
    name: String,
}

struct MainState {
    client: Client,
    game: Game,

    next_player_input: PlayerInput,

    show: Show,
    font: Font,
    fps: f64,
    show_debug: bool,
    show_profiler: bool,
}

impl MainState {
    fn update(&mut self, ctx: &mut ggez::Context) -> ggez::GameResult<()> {
        profile!("update");

        self.fps = ggez::timer::get_fps(ctx);
        let delta = ggez::timer::get_delta(ctx);

        match self.game
            .update(&mut self.client, &self.next_player_input, delta)
            .unwrap()
        {
            Some(hooks_game::game::Event::Disconnected) => {
                info!("Got disconnected! Bye.");
                ctx.quit()?;
            }
            Some(hooks_game::game::Event::TickStarted(ref events)) => {
                self.show.handle_events(self.game.world_mut(), events)?;
            }
            None => {}
        }

        Ok(())
    }

    fn draw(&mut self, ctx: &mut ggez::Context) -> ggez::GameResult<()> {
        profile!("draw");

        {
            profile!("clear");
            ggez::graphics::clear(ctx);
        }

        self.show.draw(ctx, self.game.world())?;

        if self.show_debug {
            profile!("draw text");
            hooks_show::debug::show(ctx, &self.font, &self.inspect(), Point2::new(10.0, 10.0))?;
        }

        {
            profile!("present");
            ggez::graphics::present(ctx);
        }

        Ok(())
    }

    fn handle_event(&mut self, ctx: &mut ggez::Context, event: event::Event) -> bool {
        match event {
            event::Event::Quit { .. } => return false,
            event::Event::MouseMotion { x, y, .. } => {
                let (size_x, size_y) = ggez::graphics::get_size(ctx);
                let size = Vector2::new(size_x as f32, size_y as f32);
                let clip = Vector2::new(
                    x.max(0).min(size_x as i32) as f32,
                    y.max(0).min(size_y as i32) as f32,
                );
                let shift = clip - size / 2.0;

                self.next_player_input.rot_angle = shift.y.atan2(shift.x)
            }
            event::Event::MouseButtonDown {
                mouse_btn: MouseButton::Left,
                ..
            } => {
                self.next_player_input.shoot_one = true;
            }
            event::Event::MouseButtonUp {
                mouse_btn: MouseButton::Left,
                ..
            } => {
                self.next_player_input.shoot_one = false;
            }
            event::Event::KeyDown {
                keycode: Some(keycode),
                ..
            } => match keycode {
                Keycode::W => self.next_player_input.move_forward = true,
                Keycode::S => self.next_player_input.move_backward = true,
                Keycode::F1 => self.show_debug = !self.show_debug,
                Keycode::F2 => self.show_profiler = !self.show_profiler,
                _ => {}
            },
            event::Event::KeyUp {
                keycode: Some(keycode),
                ..
            } => match keycode {
                Keycode::W => self.next_player_input.move_forward = false,
                Keycode::S => self.next_player_input.move_backward = false,
                _ => {}
            },
            _ => {}
        }

        true
    }

    pub fn run_frame(&mut self, ctx: &mut ggez::Context) -> ggez::GameResult<bool> {
        let _frame = PROFILER.with(|p| p.borrow_mut().frame());

        ctx.timer_context.tick();

        for event in event::Events::new(ctx)?.poll() {
            if !self.handle_event(ctx, event) {
                return Ok(false);
            }
        }

        self.update(ctx)?;
        self.draw(ctx)?;

        thread::yield_now();

        Ok(true)
    }
}

impl debug::Inspect for MainState {
    fn inspect(&self) -> debug::Vars {
        let mut vars = vec![
            ("fps".to_string(), self.fps.inspect()),
            ("game".to_string(), self.game.inspect()),
        ];

        if self.show_profiler {
            vars.push((
                "profiler".to_string(),
                PROFILER.with(|p| p.borrow().inspect()),
            ));
        }

        debug::Vars::Node(vars)
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
        next_player_input: PlayerInput::default(),
        show,
        font,
        fps: 0.0,
        show_debug: false,
        show_profiler: false,
    };

    while state.run_frame(ctx).unwrap() {}
}

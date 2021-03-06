extern crate env_logger;
extern crate ggez;
extern crate hooks_client;
extern crate hooks_game;
extern crate hooks_show;
#[macro_use]
extern crate hooks_util;
#[macro_use]
extern crate log;
extern crate nalgebra;
extern crate specs;

use std::{env, io, path, thread};

use nalgebra::{Point2, Point3, Vector2};

use ggez::event::{self, Keycode, MouseButton};
use ggez::graphics::{Color, Font};
use ggez::{conf, ContextBuilder};

use hooks_client::client::Client;
use hooks_client::game::Game;
use hooks_game::defs::{GameInfo, PlayerInput};
use hooks_game::physics::Position;
use hooks_game::registry::Registry;
use hooks_show::{Assets, Show};
use hooks_util::debug::{self, Inspect};
use hooks_util::profile::{self, PROFILER};
use hooks_util::stats;
use hooks_util::timer::{duration_to_secs, Stopwatch};

fn register(reg: &mut Registry, game_info: &GameInfo) {
    // Game state
    hooks_client::game::register(reg, game_info);

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
    update_stopwatch: Stopwatch,

    show: Show,
    font: Font,
    fps: f64,

    show_debug: bool,
    show_profiler: bool,
    show_stats: bool,
}

impl MainState {
    fn update(&mut self, ctx: &mut ggez::Context) -> ggez::GameResult<()> {
        profile!("update");

        self.fps = ggez::timer::get_fps(ctx);

        let mut delta = self.update_stopwatch.get_reset();

        stats::update(duration_to_secs(delta));
        stats::record("dt", duration_to_secs(delta));

        while let Some(event) = self.game
            .update(&mut self.client, &self.next_player_input, delta)
            .unwrap()
        // TODO: This is where actual error handling will need to happen
        {
            delta = self.update_stopwatch.get_reset();

            match event {
                hooks_client::game::Event::Disconnected => {
                    info!("Got disconnected! Bye.");
                    ctx.quit()?;
                }
                hooks_client::game::Event::TickStarted(ref events) => {
                    self.show.handle_events(ctx, self.game.world_mut(), events)?;
                }
            }
        }

        self.game.interpolate();

        Ok(())
    }

    fn draw(&mut self, ctx: &mut ggez::Context) -> ggez::GameResult<()> {
        profile!("draw");

        {
            profile!("clear");
            ggez::graphics::set_background_color(
                ctx,
                Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.0,
                },
            );
            ggez::graphics::clear(ctx);
        }

        self.show.draw(ctx, self.game.world())?;

        if self.show_debug {
            profile!("text");
            hooks_show::debug::show(ctx, &self.font, &self.inspect(), Point2::new(10.0, 10.0))?;
        }

        {
            profile!("present");
            ggez::graphics::present(ctx);
        }

        Ok(())
    }

    fn handle_event(&mut self, ctx: &mut ggez::Context, event: &event::Event) -> bool {
        match *event {
            event::Event::Quit { .. } => return false,
            event::Event::MouseMotion { x, y, .. } => {
                let (size_x, size_y) = ggez::graphics::get_size(ctx);
                let size = Vector2::new(size_x as f32, size_y as f32);
                let clip = Vector2::new(
                    x.max(0).min(size_x as i32) as f32,
                    y.max(0).min(size_y as i32) as f32,
                );
                let point = clip - size / 2.0;
                let world = self.game.world();
                let pos = if let Some(my_entity) = self.show.my_player_entity(world) {
                    let positions = world.read::<Position>();
                    if let Some(position) = positions.get(my_entity) {
                        let pos = Point3::new(clip.x, clip.y, 0.0) -
                            self.show.camera().similarity() *
                                Point3::new(position.0.coords.x, position.0.coords.y, 0.0);
                        Vector2::new(pos.x, pos.y)
                    } else {
                        point
                    }
                } else {
                    point
                };

                self.next_player_input.rot_angle = pos.y.atan2(pos.x)
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
            event::Event::MouseButtonDown {
                mouse_btn: MouseButton::Right,
                ..
            } => {
                self.next_player_input.shoot_two = true;
            }
            event::Event::MouseButtonUp {
                mouse_btn: MouseButton::Right,
                ..
            } => {
                self.next_player_input.shoot_two = false;
            }
            event::Event::KeyDown {
                keycode: Some(keycode),
                ..
            } => match keycode {
                Keycode::W => self.next_player_input.move_forward = true,
                Keycode::S => self.next_player_input.move_backward = true,
                Keycode::A => self.next_player_input.move_left = true,
                Keycode::D => self.next_player_input.move_right = true,
                Keycode::Q => self.next_player_input.pull_one = true,
                Keycode::E => self.next_player_input.pull_two = true,
                Keycode::F1 => self.show_debug = !self.show_debug,
                Keycode::F2 => self.show_profiler = !self.show_profiler,
                Keycode::F3 => self.show_stats = !self.show_stats,
                Keycode::P => {
                    PROFILER.with(|p| {
                        p.borrow().inspect().print(&mut io::stdout());
                        p.borrow_mut().reset();
                    });
                }
                _ => {}
            },
            event::Event::KeyUp {
                keycode: Some(keycode),
                ..
            } => match keycode {
                Keycode::W => self.next_player_input.move_forward = false,
                Keycode::S => self.next_player_input.move_backward = false,
                Keycode::A => self.next_player_input.move_left = false,
                Keycode::D => self.next_player_input.move_right = false,
                Keycode::Q => self.next_player_input.pull_one = false,
                Keycode::E => self.next_player_input.pull_two = false,
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
            if !self.handle_event(ctx, &event) {
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
            ("fps".to_string(), (self.fps as usize).inspect()),
            ("game".to_string(), self.game.inspect()),
        ];

        if self.show_profiler {
            vars.push((
                "profiler".to_string(),
                PROFILER.with(|p| p.borrow().inspect()),
            ));
        }

        if self.show_stats {
            vars.push(("stats".to_string(), stats::inspect()));
        }

        debug::Vars::Node(vars)
    }
}

fn main() {
    env_logger::init();

    let host = env::args()
        .nth(1)
        .unwrap_or_else(|| "localhost".to_string());

    let config = Config {
        host,
        port: 32444,
        name: "testy".to_string(),
    };
    let timeout_ms = 5000;

    // Initialize ggez
    let ctx = &mut ContextBuilder::new("hooks-frenzy", "leod")
        .window_mode(
            conf::WindowMode::default()
                .dimensions(1600, 900)
                //.fullscreen_type(conf::FullscreenType::True)
                .vsync(true),
        )
        .build()
        .unwrap();

    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        let mut path = path::PathBuf::from(manifest_dir);
        path.push("resources");
        ctx.filesystem.mount(&path, true);

        info!("Loading resources from {:?}", path);
    }

    // Connect to server
    let mut client = Client::connect(&config.host, config.port, &config.name, timeout_ms).unwrap();
    info!(
        "Connected to {}:{} with game info {:?}",
        config.host,
        config.port,
        client.game_info()
    );

    // Inform the server that we are good to go
    // TODO: Ready should be sent only when we have loaded all the necessary assets/maps
    //       (Show/Game require my_player_id at construction time, need to refactor)
    let my_player_id = client.ready(timeout_ms).unwrap();

    info!("Joined game with id {}", my_player_id);

    let assets = Assets::new(ctx).unwrap();
    let size = ggez::graphics::get_size(ctx);
    let show = Show::load(ctx, size, my_player_id, client.game_info(), assets).unwrap();
    let font = Font::default_font().unwrap();

    // Register and create game
    let mut reg = Registry::new();
    register(&mut reg, client.game_info());

    let game = Game::new(reg, my_player_id, client.game_info(), true);

    let mut state = MainState {
        client,
        game,
        next_player_input: PlayerInput::default(),
        update_stopwatch: Stopwatch::new(),
        show,
        font,
        fps: 0.0,
        show_debug: false,
        show_profiler: false,
        show_stats: false,
    };

    while state.run_frame(ctx).unwrap() {}
}

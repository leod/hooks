use std::io;
use std::thread;
use std::time::Duration;

use hooks_common::GameInfo;
#[cfg(feature = "show")]
use hooks_show::Show;
use hooks_util::debug::Inspect;
use hooks_util::profile::PROFILER;
use hooks_util::timer::{Stopwatch, Timer};

use game::Game;
use host::{self, Host};

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub game_info: GameInfo,
}

pub struct Server {
    profile: bool,
    profile_timer: Timer,
    stopwatch: Stopwatch,

    host: Host,
    game: Game,
    #[cfg(feature = "show")]
    show: Show,
}

impl Server {
    pub fn create(config: Config) -> Result<Server, host::Error> {
        info!(
            "Starting server on port {} with game config {:?}",
            config.port, config.game_info
        );

        let host = Host::create(config.port, config.game_info.clone())?;
        let game = Game::new(config.game_info.clone());

        Ok(Server {
            profile: true,
            profile_timer: Timer::new(Duration::from_secs(5)),
            stopwatch: Stopwatch::new(),
            host,
            game,
        })
    }

    pub fn run(&mut self) -> Result<(), host::Error> {
        loop {
            if self.profile {
                self.profile_timer += self.stopwatch.get_reset();
                if self.profile_timer.trigger_reset() {
                    PROFILER.with(|p| p.borrow().inspect().print(&mut io::stdout()));
                }
            }

            let _frame = PROFILER.with(|p| p.borrow_mut().frame());

            self.game.update(&mut self.host)?;

            thread::sleep(Duration::from_millis(1));
        }
    }
}

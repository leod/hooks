use std::thread;
use std::time::Duration;

use hooks_common::GameInfo;
#[cfg(feature = "show")]
use hooks_show::Show;

use game::Game;
use host::{self, Host};

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub game_info: GameInfo,
}

pub struct Server {
    host: Host,
    game: Game,
    #[cfg(feature = "show")] show: Show,
}

impl Server {
    pub fn create(config: Config) -> Result<Server, host::Error> {
        info!(
            "Starting server on port {} with game config {:?}",
            config.port, config.game_info
        );

        let host = Host::create(config.port, config.game_info.clone())?;
        let game = Game::new(config.game_info.clone());

        Ok(Server { host, game })
    }

    pub fn run(&mut self) -> Result<(), host::Error> {
        loop {
            self.game.update(&mut self.host)?;

            thread::sleep(Duration::from_millis(1));
        }
    }
}

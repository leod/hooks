use std::thread;

use common::GameInfo;

use host::{self, Host};

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub game_info: GameInfo,
}

pub struct Server {
    config: Config,
    host: Host,
}

impl Server {
    pub fn create(config: Config) -> Result<Server, host::Error> {
        info!(
            "Starting server on port {} with game config {:?}",
            config.port, config.game_info
        );

        let host = Host::create(config.port, config.game_info.clone())?;

        Ok(Server { config, host })
    }

    pub fn run(&mut self) -> Result<(), host::Error> {
        loop {
            if let Some(event) = self.host.service()? {
                match event {
                    host::Event::PlayerConnected(player_id, name) => {
                        info!("Player {} connected with name {}", player_id, name);
                    }
                    host::Event::PlayerDisconnected(player_id, reason) => {
                        info!("Player {} disconnected with reason {:?}", player_id, reason);
                    }
                    host::Event::ClientGameMsg(player_id, msg) => {}
                }
            }

            thread::yield_now();
        }
    }
}

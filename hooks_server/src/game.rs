use std::collections::BTreeMap;

use common::{self, event, game, GameInfo, PlayerId, PlayerInfo};
use common::registry::Registry;
use common::repl::player;

use host::{self, Host};

struct Player {}

pub struct Game {
    state: game::State,
    players: BTreeMap<PlayerId, Player>,
}

fn register(game_info: &GameInfo, reg: &mut Registry) {
    common::auth::register(game_info, reg);
}

impl Game {
    pub fn new(game_info: GameInfo) -> Game {
        let state = {
            let mut reg = Registry::new();

            register(&game_info, &mut reg);

            game::State::from_registry(reg)
        };

        Game {
            state,
            players: BTreeMap::new(),
        }
    }

    pub fn update(&mut self, host: &mut Host) -> Result<(), host::Error> {
        // 1. Handle network messages and create resulting game events
        let mut events = event::Sink::new();

        while let Some(event) = host.service()? {
            match event {
                host::Event::PlayerJoined(player_id, name) => {
                    let player_info = PlayerInfo::new(name.clone());

                    events.push(player::JoinedEvent {
                        id: player_id,
                        info: player_info,
                    });

                    assert!(!self.players.contains_key(&player_id));
                    self.players.insert(player_id, Player {});

                    info!("Player {} with name {} joined", player_id, name);
                }
                host::Event::PlayerLeft(player_id, reason) => {
                    events.push(player::LeftEvent {
                        id: player_id,
                        reason,
                    });

                    assert!(self.players.contains_key(&player_id));
                    self.players.remove(&player_id);

                    let players = self.state.world.read_resource::<player::Players>();
                    let name = if let Some(player_info) = players.get(player_id) {
                        "known name: ".to_string() + &player_info.name
                    } else {
                        // While we have received the host PlayerJoined event, game logic has not
                        // yet processed it in a tick.
                        "unknown name".to_string()
                    };

                    info!("Player {} ({}) left", player_id, name);
                }
                host::Event::ClientGameMsg(player_id, msg) => {}
            }
        }

        Ok(())
    }
}

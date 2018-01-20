use std::collections::BTreeMap;
use std::collections::btree_map;

use specs::{self, Entity, Join, World};

use defs::{LeaveReason, PlayerId, PlayerInfo};
use event::{self, Event};
use registry::Registry;
use repl::{self, entity};

pub fn register(reg: &mut Registry) {
    reg.resource(Players(BTreeMap::new()));
    reg.event::<JoinedEvent>();
    reg.event::<LeftEvent>();
    reg.event_handler_pre_tick(handle_event_pre_tick);
}

/// For each player, store information (like the name and statistics) and the current main entity
/// of the player, if it exists. Management of the player's entity handle is currently handled by
/// the `repl::entity` mod.
#[derive(Clone)]
pub struct Players(pub BTreeMap<PlayerId, (PlayerInfo, Option<specs::Entity>)>);

impl Players {
    pub fn get(&self, id: PlayerId) -> Option<&(PlayerInfo, Option<specs::Entity>)> {
        self.0.get(&id)
    }

    pub fn iter(&self) -> btree_map::Iter<PlayerId, (PlayerInfo, Option<specs::Entity>)> {
        self.0.iter()
    }
}

#[derive(Debug, Clone, BitStore)]
pub struct JoinedEvent {
    pub id: PlayerId,
    pub info: PlayerInfo,
}

impl Event for JoinedEvent {
    fn class(&self) -> event::Class {
        event::Class::Order
    }
}

#[derive(Debug, Clone, BitStore)]
pub struct LeftEvent {
    pub id: PlayerId,
    pub reason: LeaveReason,
}

impl Event for LeftEvent {
    fn class(&self) -> event::Class {
        event::Class::Order
    }
}

/// Handle events regarding player creation. Note that, both on the server and the clients, this
/// event comes from the outside. Thus, we want to handle these before starting the tick.
fn handle_event_pre_tick(world: &mut World, event: &Box<Event>) -> Result<(), repl::Error> {
    match_event!(event:
        JoinedEvent => {
            info!("Player {} with name {} joined", event.id, event.info.name);

            let mut players = world.write_resource::<Players>();

            if players.0.contains_key(&event.id) {
                // Replication error. This should not happen.
                return Err(repl::Error::InvalidPlayerId(event.id));
            }

            players.0.insert(event.id, (event.info.clone(), None));
        },
        LeftEvent => {
            {
                let players = world.read_resource::<Players>();

                if !players.0.contains_key(&event.id) {
                    // Replication error. This should not happen.
                    return Err(repl::Error::InvalidPlayerId(event.id));
                }

                info!("Player {} with name {} left", event.id, players.0[&event.id].0.name);
            }

            // Remove all entities owned by the disconnected player
            let owned_ids = {
                let mut repl_ids = world.read::<repl::Id>();
                let mut repl_entities = world.read::<repl::Entity>();

                (&repl_ids, &repl_entities).join()
                    .filter_map(|(id, entity)| {
                        if entity.owner == event.id {
                            Some(id.0)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            };

            for &id in &owned_ids {
                entity::remove(world, id);
            }

            world.write_resource::<Players>().0.remove(&event.id).unwrap();
        },
    );

    Ok(())
}

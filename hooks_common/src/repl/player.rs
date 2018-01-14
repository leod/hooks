use std::collections::BTreeMap;
use std::collections::btree_map;

use specs::{Join, World};

use defs::{LeaveReason, PlayerId, PlayerInfo};
use event::{self, Event};
use registry::Registry;
use repl::{self, entity};

pub fn register(reg: &mut Registry) {
    reg.resource(Players(BTreeMap::new()));
    reg.event::<JoinedEvent>();
    reg.event::<LeftEvent>();
    reg.event_handler_post_tick(handle_event_post_tick);
}

pub struct Players(BTreeMap<PlayerId, PlayerInfo>);

impl Players {
    pub fn get(&self, id: PlayerId) -> Option<&PlayerInfo> {
        self.0.get(&id)
    }

    pub fn iter(&self) -> btree_map::Iter<PlayerId, PlayerInfo> {
        self.0.iter()
    }
}

#[derive(Debug, BitStore)]
pub struct JoinedEvent {
    pub id: PlayerId,
    pub info: PlayerInfo,
}

impl Event for JoinedEvent {
    fn class(&self) -> event::Class {
        event::Class::Order
    }
}

#[derive(Debug, BitStore)]
pub struct LeftEvent {
    pub id: PlayerId,
    pub reason: LeaveReason,
}
impl Event for LeftEvent {
    fn class(&self) -> event::Class {
        event::Class::Order
    }
}

fn handle_event_post_tick(world: &mut World, event: &Box<Event>) -> Result<(), repl::Error> {
    match_event!(event:
        JoinedEvent => {
            info!("Player {} with name {} joined", event.id, event.info.name);

            let mut players = world.write_resource::<Players>();

            if players.0.contains_key(&event.id) {
                // Replication error. This should not happen.
                return Err(repl::Error::InvalidPlayerId(event.id));
            }

            players.0.insert(event.id, event.info.clone());
        },
        LeftEvent => {
            {
                let players = world.read_resource::<Players>();

                if !players.0.contains_key(&event.id) {
                    // Replication error. This should not happen.
                    return Err(repl::Error::InvalidPlayerId(event.id));
                }

                info!("Player {} with name {} left", event.id, players.0[&event.id].name);
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

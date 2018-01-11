use std::collections::BTreeMap;

use specs::{World, Join};

use defs::{PlayerId, PlayerInfo};
use registry::Registry;
use event::{self, Event};
use repl::{self, entity};

pub fn register(reg: &mut Registry) {
    reg.resource(Players(BTreeMap::new()));
    reg.event::<JoinedEvent>();
    reg.event::<LeftEvent>();
    reg.post_tick_event_handler(handle_post_tick_event);
}

pub struct Players(BTreeMap<PlayerId, PlayerInfo>);

impl Players {
    pub fn get(&self, id: PlayerId) -> Option<&PlayerInfo> {
        self.0.get(&id)
    }
}

#[derive(Debug, BitStore)]
pub struct JoinedEvent {
    pub id: PlayerId,
    pub info: PlayerInfo
}

impl Event for JoinedEvent {
    fn class(&self) -> event::Class {
        event::Class::Order
    }
}

#[derive(Debug, BitStore)]
pub struct LeftEvent {
    pub id: PlayerId,
}

impl Event for LeftEvent {
    fn class(&self) -> event::Class {
        event::Class::Order
    }
}

fn handle_post_tick_event(world: &mut World, event: &Box<Event>) {
    match_event!(event:
        JoinedEvent => {
            let mut players = world.write_resource::<Players>();
            
            // TODO: repl error
            assert!(!players.0.contains_key(&event.id));

            players.0.insert(event.id, event.info.clone());
        },
        LeftEvent => {
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

            // TODO: repl error
            world.write_resource::<Players>().0.remove(&event.id).unwrap();
        },
    );
}

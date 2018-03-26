use std::collections::BTreeMap;
use std::collections::btree_map;

use specs::prelude::{Entity, Join, World};

use defs::{EntityIndex, LeaveReason, PlayerId, PlayerInfo};
use event::{self, Event};
use registry::Registry;
use entity;
use repl;

pub fn register(reg: &mut Registry) {
    reg.resource(Players(BTreeMap::new()));

    reg.event::<JoinedEvent>();
    reg.event::<LeftEvent>();

    reg.event_handler_pre_tick(handle_event_pre_tick);
}

/// Information about a player that is known by the server and clients.
#[derive(Clone, Debug)]
pub struct Player {
    pub info: PlayerInfo,

    /// Current controlled entity. Maintained by `repl::entity`.
    pub entity: Option<Entity>,

    /// Index of the next entity that will be created for this player. This is used by the server
    /// for all players, and might get used by clients for prediction of the creation of owned
    /// entities.
    pub next_entity_index: EntityIndex,
}

impl Player {
    pub fn new(info: PlayerInfo) -> Player {
        Player {
            info: info,
            entity: None,
            next_entity_index: 0,
        }
    }

    pub fn advance_entity_index(&mut self) -> EntityIndex {
        let index = self.next_entity_index;
        self.next_entity_index += 1;
        index
    }

    pub fn next_entity_index(&self, n: EntityIndex) -> EntityIndex {
        self.next_entity_index + n
    }
}

/// For each player, store information (like the name and statistics) and the current main entity
/// of the player, if it exists. Management of the player's entity handle is currently handled by
/// the `repl::entity` mod.
#[derive(Clone)]
pub struct Players(pub BTreeMap<PlayerId, Player>);

impl Players {
    pub fn get(&self, id: PlayerId) -> Option<&Player> {
        self.0.get(&id)
    }

    pub fn iter(&self) -> btree_map::Iter<PlayerId, Player> {
        self.0.iter()
    }
}

pub fn get(world: &mut World, id: PlayerId) -> Option<Player> {
    let players = world.read_resource::<Players>();
    players.get(id).cloned()
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
fn handle_event_pre_tick(world: &mut World, event: &Event) -> Result<(), repl::Error> {
    match_event!(event:
        JoinedEvent => {
            info!("Player {} with name {} joined", event.id, event.info.name);

            let mut players = world.write_resource::<Players>();

            if players.0.contains_key(&event.id) {
                // Replication error. This should not happen.
                return Err(repl::Error::InvalidPlayerId(event.id));
            }

            players.0.insert(event.id, Player::new(event.info.clone()));
        },
        LeftEvent => {
            {
                let players = world.read_resource::<Players>();

                if !players.0.contains_key(&event.id) {
                    // Replication error. This should not happen.
                    return Err(repl::Error::InvalidPlayerId(event.id));
                }

                info!("Player {} with name {} left", event.id, players.0[&event.id].info.name);
            }

            // Remove all entities owned by the disconnected player
            let owned_entities = {
                let entities = world.entities();
                let mut repl_ids = world.read::<repl::Id>();

                (&*entities, &repl_ids).join()
                    .filter_map(|(entity, &repl::Id(id))| {
                        if id.0 == event.id {
                            Some(entity)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            };

            // Possible issue here: we remove the entities in a deferred way, so that systems can
            // do something when an entity is removed --- most importantly, removing it from the
            // `CollisionWorld`. But here, we remove the player immediately. Thus, everything after
            // the removal here and before the next call to `entity::perform_removals` needs to be
            // careful not to assume that the player information is still there!
            //
            // Right now, we immediately call `entity::perform_removals` after running the pre tick
            // event handlers (in `game::state::run_pre_tick`), so we should be safe unless other
            // pre tick event handlers do something stupid.

            for &id in &owned_entities {
                entity::deferred_remove(world, id);
            }

            world.write_resource::<Players>().0.remove(&event.id).unwrap();
        },
    );

    Ok(())
}

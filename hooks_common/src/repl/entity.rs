use std::collections::BTreeMap;

use specs::{self, Entity, EntityBuilder, World};

use defs::{EntityClassId, EntityId, GameInfo, PlayerId, INVALID_ENTITY_ID, INVALID_PLAYER_ID};
use event::{self, Event};
use registry::Registry;
use repl::{self, player};

pub use repl::snapshot::{ComponentType, EntityClass, EntityClasses, EntitySnapshot, WorldSnapshot};

fn register<T: EntitySnapshot>(reg: &mut Registry) {
    reg.resource(EntityClasses::<T>(BTreeMap::new()));
    reg.resource(Ctors(BTreeMap::new()));
    reg.resource(ClassNames(BTreeMap::new()));

    reg.event::<RemoveOrder>();
}

/// Event to remove entities, broadcast to clients
#[derive(Debug, Clone, BitStore)]
pub struct RemoveOrder(pub EntityId);

impl Event for RemoveOrder {
    fn class(&self) -> event::Class {
        event::Class::Order
    }
}

// TODO: Probably want to use Box<FnSomething>
pub type Ctor = fn(specs::EntityBuilder) -> specs::EntityBuilder;

/// Constructors, e.g. for adding client-side-specific components to replicated entities.
struct Ctors(pub BTreeMap<EntityClassId, Vec<Ctor>>);

/// Maps from entity class names to their unique id. This map should be exactly the same on server
/// and clients and not change during a game.
struct ClassNames(pub BTreeMap<String, EntityClassId>);

/// Register a new entity class. This should only be called in register functions that are used by
/// both the server and the clients. Server and clients can attach their specific entity
/// constructors locally via `add_ctor`.
///
/// Note that this function must only be called after this module's register function.
pub fn register_type<T: ComponentType>(
    reg: &mut Registry,
    name: &str,
    components: &[T],
    ctor: Ctor,
) -> EntityClassId {
    let world = reg.world();

    let mut classes = world.write_resource::<EntityClasses<T::EntitySnapshot>>();
    let mut ctors = world.write_resource::<Ctors>();
    let mut class_names = world.write_resource::<ClassNames>();

    let class_id = classes.0.keys().next_back().map(|&id| id + 1).unwrap_or(0);

    info!(
        "Registering entity type {} with id {} and repl components {:?}",
        name, class_id, components,
    );

    assert!(!classes.0.contains_key(&class_id));
    assert!(!ctors.0.contains_key(&class_id));
    assert!(!class_names.0.values().any(|&id| id == class_id));

    let class = EntityClass::<T::EntitySnapshot> {
        components: components.to_vec(),
    };

    classes.0.insert(class_id, class);
    ctors.0.insert(class_id, vec![ctor]);
    class_names.0.insert(name.to_string(), class_id);

    assert!(classes.0.len() == ctors.0.len());
    assert!(ctors.0.len() == class_names.0.len());

    class_id
}

pub fn add_ctor(reg: &mut Registry, name: &str, ctor: Ctor) {
    let world = reg.world();

    let class_id = {
        let class_names = world.read_resource::<ClassNames>();
        class_names.0[name]
    };

    let mut ctors = world.write_resource::<Ctors>();
    let ctor_vec = ctors.0.get_mut(&class_id).unwrap();
    ctor_vec.push(ctor);
}

fn create<F>(
    world: &mut World,
    id: EntityId,
    owner: PlayerId,
    class_id: EntityClassId,
    ctor: F,
) -> Result<(EntityId, specs::Entity), repl::Error>
where
    F: FnOnce(EntityBuilder) -> EntityBuilder,
{
    debug!(
        "Creating entity {} for player {} of type {}",
        id, owner, class_id
    );

    let ctors = {
        let ctors = world.read_resource::<Ctors>();

        if let Some(ctor) = ctors.0.get(&class_id) {
            ctor.clone()
        } else {
            return Err(repl::Error::InvalidEntityClassId(class_id));
        }
    };

    // Build entity
    let entity = {
        let builder = world
            .create_entity()
            .with(repl::Id(id))
            .with(repl::Entity { owner, class_id });

        let builder = ctors.iter().fold(builder, |builder, ctor| ctor(builder));

        // Custom constructor
        let builder = ctor(builder);

        builder.build()
    };

    // Remember player-controlled main entity
    // TODO: Should this be here? First, I thought I could handle this in the `player` mod by
    //       emitting a local `Created` event here. However, this leads to some headaches due to
    //       event orders. Consider e.g. the situation that we spawn an entity for a player,
    //       but then during the same tick that player disconnects. Then we would need to take care
    //       to somehow insert the `Created` event *inbetween* the `player::JoinedEvent` and
    //       `player::LeftEvent` (or ignore `Created` events with invalid player ids).
    //       For now, let's just handle this immediately when the entity is created or removed.
    if owner != INVALID_PLAYER_ID {
        let game_info = world.read_resource::<GameInfo>();
        let class_names = world.read_resource::<ClassNames>();

        let mut players = world.write_resource::<player::Players>();

        let player = if let Some(player) = players.0.get_mut(&owner) {
            player
        } else {
            return Err(repl::Error::InvalidPlayerId(owner));
        };
        let player_class_id =
            if let Some(&player_class_id) = class_names.0.get(&game_info.player_entity_class) {
                player_class_id
            } else {
                return Err(repl::Error::InvalidEntityClass(
                    game_info.player_entity_class.clone(),
                ));
            };

        if class_id == player_class_id {
            if player.1.is_some() {
                return Err(repl::Error::Replication(format!(
                    "player {} already has a main entity with id",
                    id
                )));
            }

            player.1 = Some(entity);
        }
    }

    // Map from shared id to ECS handle
    {
        let mut entity_map = world.write_resource::<repl::EntityMap>();
        assert!(!entity_map.0.contains_key(&id), "entity id used twice");

        entity_map.0.insert(id, entity);
    }

    Ok((id, entity))
}

pub(super) fn remove(world: &mut World, id: EntityId) -> Result<(), repl::Error> {
    debug!("Removing entity {}", id);

    let entity = {
        let mut entity_map = world.write_resource::<repl::EntityMap>();
        let entity = entity_map.id_to_entity(id);
        entity_map.0.remove(&id);
        entity
    };

    // Remember player-controlled main entity
    let repl_entity = world.read::<repl::Entity>().get(entity).unwrap().clone();
    if repl_entity.owner != INVALID_PLAYER_ID {
        let game_info = world.read_resource::<GameInfo>();
        let class_names = world.read_resource::<ClassNames>();

        let mut players = world.write_resource::<player::Players>();

        let player = if let Some(player) = players.0.get_mut(&repl_entity.owner) {
            player
        } else {
            return Err(repl::Error::InvalidPlayerId(repl_entity.owner));
        };
        let player_class_id =
            if let Some(player_class_id) = class_names.0.get(&game_info.player_entity_class) {
                *player_class_id
            } else {
                return Err(repl::Error::InvalidEntityClass(
                    game_info.player_entity_class.clone(),
                ));
            };

        if repl_entity.class_id == player_class_id {
            if player.1.is_none() {
                return Err(repl::Error::Replication(format!(
                    "player {} has no main entity to remove",
                    repl_entity.owner
                )));
            }

            player.1 = None;
        }
    }

    world.delete_entity(entity).unwrap();

    Ok(())
}

/// Server-side entity management
pub mod auth {
    use super::*;

    pub fn register<T: EntitySnapshot>(reg: &mut Registry) {
        super::register::<T>(reg);

        reg.resource(IdSource {
            next_id: INVALID_ENTITY_ID + 1,
        });
    }

    struct IdSource {
        next_id: EntityId,
    }

    impl IdSource {
        fn next_id(&mut self) -> EntityId {
            let id = self.next_id;
            self.next_id += 1;

            assert!(id != INVALID_ENTITY_ID);

            id
        }
    }

    pub fn create<F>(world: &mut World, owner: PlayerId, class: &str, ctor: F) -> (EntityId, Entity)
    where
        F: FnOnce(EntityBuilder) -> EntityBuilder,
    {
        let id = {
            let mut id_source = world.write_resource::<IdSource>();
            id_source.next_id()
        };

        let class_id = {
            let class_names = world.read_resource::<ClassNames>();
            class_names.0[class]
        };

        super::create(world, id, owner, class_id, ctor).unwrap() // On the server, replication errors are definitely a bug
    }
}

/// Client-side entity management
pub mod view {
    use ordered_join;
    use repl;

    use super::*;

    pub fn register<T: EntitySnapshot>(reg: &mut Registry) {
        super::register::<T>(reg);
    }

    /// Create entities that are new in this snapshot. Note that this doesn't mean that the entity
    /// was created in this snapshot, but it is the first time that this client sees it.
    ///
    /// Snapshot data of new entities is not loaded here.
    pub fn create_new_entities<T: EntitySnapshot>(
        world: &mut World,
        snapshot: &WorldSnapshot<T>,
    ) -> Result<(), repl::Error> {
        let new_entities = {
            let entity_map = world.read_resource::<repl::EntityMap>();

            ordered_join::FullJoinIter::new(entity_map.0.iter(), snapshot.0.iter())
                .filter_map(|item| match item {
                    ordered_join::Item::Right(&id, entity_snapshot) => {
                        Some((id, entity_snapshot.clone()))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };

        for &(id, (ref repl_entity, ref _snapshot)) in &new_entities {
            debug!("Replicating entity {} of type {}", id, repl_entity.class_id);

            super::create(
                world,
                id,
                repl_entity.owner,
                repl_entity.class_id,
                |builder| builder,
            )?;
        }

        Ok(())
    }

    /// Remove entities as ordered.
    pub fn handle_event(world: &mut World, event: &Box<Event>) -> Result<(), repl::Error> {
        match_event!(event:
            RemoveOrder => {
                let id = event.0;

                let entity = {
                    let entity_map = world.read_resource::<repl::EntityMap>();
                    entity_map.get_id_to_entity(id)
                };

                if let Some(_) = entity {
                    debug!("Removing entity {}", id);

                    super::remove(world, id)?;
                } else {
                    // TODO: Is it a replication error if we get a `RemoveOrder` for an entity we 
                    //       don't have? This really depends on if we do both of the following:
                    //       1. Only send a subset of entities to clients.
                    //       2. Send `RemoveOrder` even if we have never shown this entity to the
                    //          client.
                    //       For now, let's say we can just ignore it.
                    //
                    //       On second thought, this can also happen if we get the `RemoveOrder`
                    //       for an entity in an intermediate tick that we did not receive.
                    //       Could the server filter for this as well?
                    warn!("Received `RemoveOrder` for entity {}, which we do not have", id);
                }
            },
        );

        Ok(())
    }
}

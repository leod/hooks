use std::collections::BTreeMap;

use specs::prelude::*;

use defs::{EntityClassId, EntityId, EntityIndex, GameInfo, PlayerId, INVALID_PLAYER_ID};
use entity;
use event::{self, Event};
use registry::Registry;
use repl::{self, player};

pub use entity::Meta;
pub use repl::snapshot::{ComponentType, EntityClass, EntityClasses, EntitySnapshot, WorldSnapshot};

fn register<T: EntitySnapshot>(reg: &mut Registry) {
    reg.resource(EntityClasses::<T>(BTreeMap::new()));

    reg.event::<RemoveOrder>();

    reg.removal_system(RemovalSys, "repl::entity");

    // Index source for entities not owned by a player
    reg.resource(auth::IndexSource { next: 1 });
}

/// Event to remove entities, broadcast to clients
#[derive(Debug, Clone, BitStore)]
pub struct RemoveOrder(pub EntityId);

impl Event for RemoveOrder {
    fn class(&self) -> event::Class {
        event::Class::Order
    }
}

/// Register a new entity class. This should only be called in register functions that are used by
/// both the server and the clients. Server and clients can attach their specific entity
/// constructors locally via `entity::add_ctor`.
///
/// Note that this function must only be called after this module's register function.
pub fn register_class<T: ComponentType>(
    reg: &mut Registry,
    name: &str,
    repl_components: &[T],
    ctor: entity::Ctor,
) -> EntityClassId {
    let class_id = entity::register_class(reg, name, ctor);

    info!(
        "Registering replicated entity class {} with id {} and repl components {:?}",
        name, class_id, repl_components,
    );

    let mut classes = reg.world()
        .write_resource::<EntityClasses<T::EntitySnapshot>>();

    let class = EntityClass::<T::EntitySnapshot> {
        components: repl_components.to_vec(),
    };

    classes.0.insert(class_id, class);

    class_id
}

fn try_get_class_id(world: &World, name: &str) -> Result<EntityClassId, repl::Error> {
    if let Some(class_id) = entity::get_class_id(world, name) {
        Ok(class_id)
    } else {
        Err(repl::Error::InvalidEntityClass(name.to_string()))
    }
}

/// Create an entity with a shared, replicated id.
fn create<F>(
    world: &mut World,
    id: EntityId,
    class_id: EntityClassId,
    ctor: F,
) -> Result<Entity, repl::Error>
where
    F: FnOnce(EntityBuilder) -> EntityBuilder,
{
    if !entity::is_class_id_valid(world, class_id) {
        return Err(repl::Error::InvalidEntityClassId(class_id));
    }

    let entity = entity::create(world, class_id, |builder| ctor(builder).with(repl::Id(id)));

    // Remember player-controlled main entity
    if id.0 != INVALID_PLAYER_ID {
        let game_info = world.read_resource::<GameInfo>();
        let player_class_id = try_get_class_id(world, &game_info.player_entity_class)?;

        if class_id == player_class_id {
            debug!("Spawning {:?}", id);

            let mut players = world.write_resource::<player::Players>();
            let player = players
                .0
                .get_mut(&id.0)
                .map(Ok)
                .unwrap_or(Err(repl::Error::InvalidPlayerId(id.0)))?;

            if player.entity.is_some() {
                return Err(repl::Error::Replication(format!(
                    "player {} already has a main entity: {:?}",
                    id.0, player.entity,
                )));
            }

            player.entity = Some(entity);
        }
    }

    // Map from shared id to ECS handle
    {
        let mut entity_map = world.write_resource::<repl::EntityMap>();
        assert!(!entity_map.0.contains_key(&id), "entity id used twice");

        entity_map.0.insert(id, entity);
    }

    debug!(
        "Created entity {:?} (local index {:?}) of type {}",
        id, entity, class_id
    );

    Ok(entity)
}

struct RemovalSys;

impl<'a> System<'a> for RemovalSys {
    type SystemData = (
        Fetch<'a, GameInfo>,
        Fetch<'a, entity::ClassIds>,
        FetchMut<'a, repl::EntityMap>,
        FetchMut<'a, player::Players>,
        ReadStorage<'a, repl::Id>,
        ReadStorage<'a, entity::Meta>,
        ReadStorage<'a, entity::Remove>,
    );

    #[cfg_attr(rustfmt, rustfmt_skip)] // rustfmt bug
    fn run(
        &mut self,
        (
            game_info,
            class_ids,
            mut entity_map,
            mut players,
            repl_id,
            meta,
            remove
        ): Self::SystemData
    ) {
        for (repl_id, meta, _) in (&repl_id, &meta, &remove).join() {
            debug!("Removing repl entity {:?}", repl_id.0);

            entity_map.0.remove(&repl_id.0);

            // Forget player-controlled main entity
            if (repl_id.0).0 != INVALID_PLAYER_ID {
                let player_class_id = *class_ids.0.get(&game_info.player_entity_class).unwrap();

                if meta.class_id == player_class_id {
                    // We might have the case that the owner has just disconnected
                    if let Some(player) = players.0.get_mut(&(repl_id.0).0) {
                        debug!("Despawning {:?}", repl_id.0);

                        assert!(player.entity.is_some());
                        player.entity = None;
                    }
                }
            }
        }
    }
}

/// Server-side entity management
pub mod auth {
    use super::*;

    pub fn register<T: EntitySnapshot>(reg: &mut Registry) {
        super::register::<T>(reg);

        reg.removal_system(RemovalSys, "repl::auth::entity");
    }

    /// Send out an `RemoveOrder` when replicated entities are removed on the server.
    struct RemovalSys;

    impl<'a> System<'a> for RemovalSys {
        type SystemData = (
            FetchMut<'a, event::Sink>,
            ReadStorage<'a, repl::Id>,
            ReadStorage<'a, entity::Remove>,
        );

        #[cfg_attr(rustfmt, rustfmt_skip)] // rustfmt bug
        fn run(&mut self, (mut events, repl_id, remove): Self::SystemData) {
            for (repl_id, _) in (&repl_id, &remove).join() {
                events.push(RemoveOrder(repl_id.0));
            }
        }
    }

    pub(super) struct IndexSource {
        pub(super) next: EntityIndex,
    }

    impl IndexSource {
        fn advance_index(&mut self) -> EntityIndex {
            let index = self.next;
            self.next += 1;

            index
        }
    }

    /// Create a new entity on the server side. Here, it is possible to pass a custom constructor
    /// that can for example spawn the entity at some given position.
    pub fn create<F>(world: &mut World, owner: PlayerId, class: &str, ctor: F) -> (EntityId, Entity)
    where
        F: FnOnce(EntityBuilder) -> EntityBuilder,
    {
        // Every player has his own entity counter
        let index = if owner != INVALID_PLAYER_ID {
            let mut players = world.write_resource::<player::Players>();
            let mut player = players.0.get_mut(&owner).unwrap();
            player.advance_entity_index()
        } else {
            let mut index_source = world.write_resource::<IndexSource>();
            index_source.advance_index()
        };

        let id = (owner, index);
        let class_id = entity::get_class_id(world, class).unwrap();
        let entity = super::create(world, id, class_id, ctor);

        // On the server, replication errors are definitely a bug, so unwrap
        (id, entity.unwrap())
    }
}

/// Client-side entity management
pub mod view {
    use hooks_util::join;
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

            join::FullJoinIter::new(entity_map.0.iter(), snapshot.0.iter())
                .filter_map(|item| match item {
                    join::Item::Right(&id, entity_snapshot) => Some((id, entity_snapshot.clone())),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };

        for &(id, (ref meta, ref _snapshot)) in &new_entities {
            debug!("Replicating entity {:?} of type {}", id, meta.class_id);

            super::create(world, id, meta.class_id, |builder| builder)?;
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

                if let Some(entity) = entity {
                    debug!("Removing entity {:?}", id);

                    entity::deferred_remove(world, entity);
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
                    warn!("Received `RemoveOrder` for entity {:?}, which we do not have", id);
                }
            },
        );

        Ok(())
    }
}

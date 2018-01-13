use std::collections::BTreeMap;

use specs::{self, Entity, EntityBuilder, World};

use defs::{EntityClassId, EntityId, PlayerId, INVALID_ENTITY_ID};
use event::{self, Event};
use registry::Registry;
use repl;

pub use repl::snapshot::{ComponentType, EntityClass, EntityClasses, EntitySnapshot, WorldSnapshot};

fn register<T: EntitySnapshot>(reg: &mut Registry) {
    reg.resource(EntityClasses::<T>(BTreeMap::new()));
    reg.resource(Ctors(BTreeMap::new()));
    reg.resource(ClassNames(BTreeMap::new()));

    reg.event::<RemoveOrder>();
}

#[derive(Debug, BitStore)]
pub struct RemoveOrder(pub EntityId);

impl Event for RemoveOrder {
    fn class(&self) -> event::Class {
        event::Class::Order
    }
}

pub type Ctor = fn(specs::EntityBuilder) -> specs::EntityBuilder;

/// Constructors for adding client-side-specific components to replicated entities.
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
    name: &str,
    components: Vec<T>,
    ctor: Ctor,
    reg: &mut Registry,
) -> EntityClassId {
    let world = reg.world();

    let mut classes = world.write_resource::<EntityClasses<T::EntitySnapshot>>();
    let mut ctors = world.write_resource::<Ctors>();
    let mut class_names = world.write_resource::<ClassNames>();

    let class_id = classes.0.keys().next_back().cloned().unwrap_or(0);

    assert!(!classes.0.contains_key(&class_id));
    assert!(!ctors.0.contains_key(&class_id));
    assert!(!class_names.0.values().any(|&id| id == class_id));

    let class = EntityClass::<T::EntitySnapshot> {
        components: components,
    };

    classes.0.insert(class_id, class);
    ctors.0.insert(class_id, vec![ctor]);
    class_names.0.insert(name.to_string(), class_id);

    assert!(classes.0.len() == ctors.0.len());
    assert!(ctors.0.len() == class_names.0.len());

    class_id
}

pub fn add_ctor(name: &str, ctor: Ctor, reg: &mut Registry) {
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
) -> (EntityId, specs::Entity)
where
    F: FnOnce(EntityBuilder) -> EntityBuilder,
{
    let ctors = {
        let ctors = world.read_resource::<Ctors>();
        ctors.0[&class_id].clone()
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

    // Map from shared id to ECS handle
    {
        let mut entity_map = world.write_resource::<repl::EntityMap>();
        assert!(!entity_map.0.contains_key(&id), "entity id used twice");

        entity_map.0.insert(id, entity);
    }

    (id, entity)
}

pub(super) fn remove(world: &mut World, id: EntityId) {
    let entity = {
        let mut entity_map = world.write_resource::<repl::EntityMap>();
        entity_map.0.remove(&id);
        entity_map.id_to_entity(id)
    };
    world.delete_entity(entity).unwrap();
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

        super::create(world, id, owner, class_id, ctor)
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
    pub fn create_new_entities<T: EntitySnapshot>(world: &mut World, snapshot: &WorldSnapshot<T>) {
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
            super::create(
                world,
                id,
                repl_entity.owner,
                repl_entity.class_id,
                |builder| builder,
            );
        }
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
                    world.delete_entity(entity).unwrap();

                    let mut entity_map = world.write_resource::<repl::EntityMap>();
                    entity_map.0.remove(&id);
                }

                // TODO: Is it a replication error if we get a RemoveOrder for an entity we don't
                // have? For now, let's say we can just ignore it.
            },
        );

        Ok(())
    }
}

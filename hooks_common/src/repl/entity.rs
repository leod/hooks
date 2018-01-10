use std::collections::BTreeMap;

use specs::{self, Entity, EntityBuilder, World};

use defs::{EntityClassId, EntityId, PlayerId};
use event::{self, Event, EventBox};
use registry::Registry;
use repl;

pub use self::snapshot::{EntityClasses, EntitySnapshot, WorldSnapshot};

pub fn register(reg: &mut Registry) {
    reg.resource(repl::snapshot::EntityClasses::<EntitySnapshot>(
        BTreeMap::new(),
    ));
    reg.resource(EntityClassNames(BTreeMap::new()));

    reg.event::<RemoveOrder>();
}

snapshot! {
    use physics::Position;
    use physics::Orientation;

    mod snapshot {
        position: Position,
        orientation: Orientation,
    }
}

#[derive(Debug, BitStore)]
pub struct RemoveOrder(pub EntityId);

impl Event for RemoveOrder {
    fn class(&self) -> event::Class {
        event::Class::Order
    }
}

/// Maps from entity class names to their unique id. This map should be exactly the same on server
/// and clients and not change during a game.
pub struct EntityClassNames(pub BTreeMap<String, EntityClassId>);

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
    // Sanity check
    {
        let classes = world.read_resource::<EntityClasses>();
        assert!(classes.0.contains_key(&class_id), "unknown entity class");
    }

    // Build entity
    let entity = {
        let builder = world
            .create_entity()
            .with(repl::Id(id))
            .with(repl::Entity { owner, class_id });

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

fn remove(world: &mut World, id: EntityId) {
    let entity = {
        let mut entity_map = world.write_resource::<repl::EntityMap>();
        entity_map.0.remove(&id);
        entity_map.id_to_entity(id)
    };
    world.delete_entity(entity).unwrap();
}

/// Server-side entity management
mod auth {
    use super::*;

    pub fn register(reg: &mut Registry) {
        super::register(reg);

        reg.resource(IdSource { next_id: 0 });
    }

    struct IdSource {
        next_id: EntityId,
    }

    impl IdSource {
        fn next_id(&mut self) -> EntityId {
            let id = self.next_id;
            self.next_id += 1;

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
            let class_names = world.read_resource::<EntityClassNames>();
            class_names.0[class]
        };

        super::create(world, id, owner, class_id, ctor)
    }
}
/// Client-side entity management
mod view {
    use ordered_join;
    use repl;

    use super::*;

    pub fn register(reg: &mut Registry) {
        reg.resource(EntityCtors(BTreeMap::new()));
    }

    pub type EntityCtor = fn(specs::EntityBuilder) -> specs::EntityBuilder;

    /// Constructors for adding client-side-specific components to replicated entities.
    pub struct EntityCtors(pub BTreeMap<EntityClassId, EntityCtor>);

    /// Create entities that are new in this snapshot. Note that this doesn't mean that the entity
    /// was created in this snapshot, but it is the first time that this client sees it.
    ///
    /// Snapshot data of new entities is not loaded here.
    pub fn create_new_entities(world: &mut World, snapshot: &WorldSnapshot) {
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
            let ctor = {
                let ctors = world.read_resource::<EntityCtors>();
                ctors.0[&repl_entity.class_id]
            };

            super::create(
                world,
                id,
                repl_entity.owner,
                repl_entity.class_id,
                ctor
            );
        }
    }

    /// Remove entities as ordered.
    pub fn handle_event(world: &mut World, event: &EventBox) {
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
            },
        );
    }
}

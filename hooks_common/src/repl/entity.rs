use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::fmt;

use shred::Resources;
use specs::{self, Entities, Fetch, FetchMut, SystemData, WriteStorage, World};

use defs::{EntityClassId, EntityId, PlayerId};
use event::{self, Event, EventBox};
use ordered_join;
use registry::Registry;
use super::snapshot;

pub use self::snap::{EntityClasses, EntitySnapshot, WorldSnapshot};

snapshot! {
    use physics::Position;
    use physics::Orientation;

    mod snap {
        position: Position,
        orientation: Orientation,
    }
}

#[derive(Debug, BitStore)]
pub struct RemoveOrder(pub EntityId);

impl Event for RemoveOrder {
    fn class(&self) -> event::Class { event::Class::Order }
}

pub type EntityCtor = Fn(specs::EntityBuilder) -> specs::EntityBuilder + Sync + Send;

pub struct EntityCtors(pub BTreeMap<EntityClassId, Box<EntityCtor>>);

pub fn register(reg: &mut Registry) {
    reg.resource(snapshot::EntityClasses::<EntitySnapshot>(BTreeMap::new()));
    reg.resource(EntityCtors(BTreeMap::new()));

    reg.event::<RemoveOrder>();
}

struct EntitiesAuth {
    next_id: EntityId,
}

#[derive(SystemData)]
pub struct CreationData<'a> {
    classes: Fetch<'a, EntityClasses>,
    entity_map: FetchMut<'a, super::Entities>,
    events: FetchMut<'a, event::Sink>,
    entities: Entities<'a>,
    repl_ids: WriteStorage<'a, super::Id>,
    repl_entities: WriteStorage<'a, super::Entity>,
}

#[derive(SystemData)]
pub struct RemovalData<'a> {
    entity_map: FetchMut<'a, super::Entities>,
    events: FetchMut<'a, event::Sink>,
    entities: Entities<'a>,
}

impl EntitiesAuth {
    pub fn new() -> Self {
        Self { next_id: 0 }
    }

    pub fn create(
        &mut self,
        owner: PlayerId,
        class_id: EntityClassId,
        mut data: CreationData,
    ) -> (EntityId, specs::Entity) {
        assert!(
            data.classes.0.contains_key(&class_id),
            "unknown entity class"
        );

        let id = self.next_id;
        self.next_id += 1;
        assert!(
            !data.entity_map.map.contains_key(&id),
            "entity id used twice"
        );

        let entity = data.entities.create();
        data.repl_ids.insert(entity, super::Id(id));
        data.repl_entities.insert(
            entity,
            super::Entity {
                owner: owner,
                class_id: class_id,
            },
        );

        data.entity_map.map.insert(id, entity);

        (id, entity)
    }

    pub fn remove(&mut self, id: EntityId, mut data: RemovalData) {
        let entity = *data.entity_map.map.get(&id).unwrap();
        data.entity_map.map.remove(&id);
        data.entities.delete(entity);

        data.events.push(RemoveOrder(id));
    }
}

#[derive(SystemData)]
pub struct EventHandlingData<'a> {
    entity_map: FetchMut<'a, super::Entities>,
    entities: Entities<'a>,
    repl_ids: WriteStorage<'a, super::Id>,
    repl_entities: WriteStorage<'a, super::Entity>,
}

pub fn handle_entity_events_view(
    events: &[EventBox],
    snapshot: &WorldSnapshot,
    world: &mut World,
    //mut data: EventHandlingData,
) {
    let mut data = EventHandlingData::fetch(&world.res, 0);

    // Create entities that are new in this snapshot. Note that this doesn't mean that the entity
    // was created in this snapshot, but it is the first time that this client sees it.
    let new_entities =
        ordered_join::FullJoinIter::new(data.entity_map.map.iter(), snapshot.0.iter())
            .filter_map(|item| match item {
                ordered_join::Item::Right(&id, entity_snapshot) => {
                    // TODO: It's not clear to me why .clone() is necessary here
                    Some((id, entity_snapshot.clone()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();

    for &(id, (ref repl_entity, ref snapshot)) in &new_entities {
        let entity = data.entities.create();
        data.repl_ids.insert(entity, super::Id(id));
        data.repl_entities.insert(entity, repl_entity.clone());
        data.entity_map.map.insert(id, entity);
    }

    // Remove entities by event
    for event in events {
        match_event!(event:
            RemoveOrder => {
                let id = event.0;

                if let Some(entity) = data.entity_map.get_id_to_entity(id) {
                    data.entity_map.map.remove(&id);
                    data.entities.delete(entity);
                }
            },
        );
    }
}

use std::collections::BTreeSet;

use specs::{self, Entities, Fetch, FetchMut, SystemData, WriteStorage};

use defs::{EntityClassId, EntityId, PlayerId};
use event::{self, EventBox};
use ordered_join;

pub use self::snapshot::{EntityClasses, EntitySnapshot, WorldSnapshot};

snapshot! {
    use physics::Position;
    use physics::Orientation;

    mod snapshot {
        position: Position,
        orientation: Orientation,
    }
}

#[derive(Debug, BitStore)]
pub struct RemoveEvent(pub EntityId);

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

        data.events.push(RemoveEvent(id));
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
    mut data: EventHandlingData,
) {
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
            RemoveEvent => {
                let id = event.0;

                if let Some(entity) = data.entity_map.get_id_to_entity(id) {
                    data.entity_map.map.remove(&id);
                    data.entities.delete(entity);
                }
            },
        );
    }
}

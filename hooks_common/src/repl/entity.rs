use std::collections::BTreeSet;

use specs::{self, Entities, Fetch, FetchMut, SystemData, WriteStorage};

use defs::{EntityClassId, EntityId, PlayerId};
use event::{self, EventBox};

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
pub struct CreateEvent(pub EntityId);

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

        data.events.push(CreateEvent(id));

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
    for event in events {
        match_event!(event:
            CreateEvent => {
                let id = event.0;

                // Since we delta encode intermediate events together with the current state, it
                // can happen that we receive a creation event, but the corresponding snapshot does
                // not contain the entity. The only reasonable cause for this is that the entity
                // does not exist anymore in the server's current tick. This means that we need to
                // ignore any events for entities that we have not replicated, everywhere.
                //
                // TODO: There are two alternatives:
                // 1. Have the CreateEvent contain the initial EntitySnapshot, instead of sending
                //    it with the WorldSnapshot. Then we always have initial state for the repl
                //    entity, even if it later has been removed by the auth.
                // 2. Temporarily create the entity, but do not display it (since we don't have any
                //    e.g. position information).
                if let Some(&(ref repl_entity, ref _state)) = snapshot.0.get(&id) {
                    let entity = data.entities.create();
                    data.repl_ids.insert(entity, super::Id(id));
                    data.repl_entities.insert(entity, repl_entity.clone());

                    data.entity_map.map.insert(id, entity);

                    // We have now created the entity locally, but its EntitySnapshot has not been
                    // loaded yet. This will be done only after all events have been handled,
                    // together with loading the state of already existing repl entities.
                }
            },
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

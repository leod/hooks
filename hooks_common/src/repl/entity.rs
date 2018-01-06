use specs::{self, Entities, Fetch, FetchMut, SystemData, WriteStorage};
use std::ops::DerefMut;

use defs::{EntityClassId, EntityId, PlayerId};
use event;

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

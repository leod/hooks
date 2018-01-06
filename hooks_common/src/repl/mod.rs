#[macro_use]
mod snapshot;
mod tick;
#[cfg(test)]
mod tests;
mod entity;

use std::collections::BTreeMap;

use specs;

use defs::{EntityClassId, EntityId, PlayerId};

/// Shared entity Id for replication.
#[derive(PartialEq, Component)]
#[component(VecStorage)]
pub struct Id(EntityId);

/// Meta-information about replicated entities.
#[derive(Clone, PartialEq, Component, BitStore)]
#[component(VecStorage)]
pub struct Entity {
    pub owner: PlayerId,
    pub class_id: EntityClassId,
}

/// Map from shared EntityId to the local ECS handle.
pub struct Entities {
    map: BTreeMap<EntityId, specs::Entity>,
}

impl Entities {
    pub fn id_to_entity(&self, id: EntityId) -> specs::Entity {
        *self.map.get(&id).unwrap()
    }

    pub fn get_id_to_entity(&self, id: EntityId) -> Option<specs::Entity> {
        self.map.get(&id).map(|k| *k)
    }
}

#[macro_use]
pub mod snapshot;
pub mod tick;
pub mod entity;
pub mod player;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use specs;

use defs::{EntityClassId, EntityId, PlayerId};
use registry::Registry;

pub fn register(reg: &mut Registry) {
    reg.component::<Id>();
    reg.component::<Entity>();

    reg.resource(EntityMap(BTreeMap::new()));
}

/// Shared entity Id for replication.
#[derive(PartialEq, Component)]
#[component(VecStorage)]
pub struct Id(pub EntityId);

/// Meta-information about replicated entities.
#[derive(Clone, PartialEq, Component, BitStore)]
#[component(VecStorage)]
pub struct Entity {
    pub class_id: EntityClassId,
}

/// Map from shared EntityId to the local ECS handle.
pub struct EntityMap(BTreeMap<EntityId, specs::Entity>);

impl EntityMap {
    pub fn id_to_entity(&self, id: EntityId) -> specs::Entity {
        self.0[&id]
    }

    pub fn get_id_to_entity(&self, id: EntityId) -> Option<specs::Entity> {
        self.0.get(&id).cloned()
    }
}

/// An `Error` indicates that something went seriously wrong in replication. Either we have a bug,
/// or the server sent us an invalid snapshot. It is not possible to recover from this, so we
/// should disconnect if such an error occurs.
#[derive(Debug)]
pub enum Error {
    InvalidPlayerId(PlayerId),
    InvalidEntityClassId(EntityClassId),
    InvalidEntityClass(String),
    InvalidEntityId(EntityId),
    Replication(String),
}

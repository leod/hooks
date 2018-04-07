#[macro_use]
pub mod snapshot;
pub mod entity;
pub mod interp;
pub mod player;
pub mod tick;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::intrinsics::type_name;
use std::ops::{Deref, DerefMut};

use specs::join::JoinIter;
use specs::prelude::{Component, Entities, Entity, Join, Storage, VecStorage, World};
use specs::storage::MaskedStorage;

use defs::{EntityClassId, EntityId, PlayerId};
use registry::Registry;

pub fn register(reg: &mut Registry) {
    reg.component::<Id>();

    reg.resource(EntityMap(BTreeMap::new()));

    player::register(reg);
}

/// Trait that needs to be implemented by components that want to be replicated.
pub trait Predictable {
    fn distance(&self, _other: &Self) -> f32 {
        0.0
    }
}

/// Shared entity id for replication.
#[derive(PartialEq, Component)]
#[storage(VecStorage)]
pub struct Id(pub EntityId);

/// Map from shared EntityId to the local ECS handle.
pub struct EntityMap(BTreeMap<EntityId, Entity>);

impl EntityMap {
    pub fn id_to_entity(&self, id: EntityId) -> Entity {
        self.0[&id]
    }

    pub fn get_id_to_entity(&self, id: EntityId) -> Option<Entity> {
        self.0.get(&id).cloned()
    }

    pub fn try_id_to_entity(&self, id: EntityId) -> Result<Entity, Error> {
        self.get_id_to_entity(id)
            .map(Ok)
            .unwrap_or(Err(Error::InvalidEntityId(id)))
    }

    pub fn is_entity(&self, id: EntityId) -> bool {
        self.get_id_to_entity(id).is_some()
    }
}

pub fn get_id_to_entity(world: &World, id: EntityId) -> Option<Entity> {
    world.read_resource::<EntityMap>().get_id_to_entity(id)
}

pub fn try_id_to_entity(world: &World, id: EntityId) -> Result<Entity, Error> {
    world.read_resource::<EntityMap>().try_id_to_entity(id)
}

pub fn is_entity(world: &World, id: EntityId) -> bool {
    world.read_resource::<EntityMap>().is_entity(id)
}

pub fn try_component<'a, T, D>(storage: &'a Storage<T, D>, entity: Entity) -> Result<&'a T, Error>
where
    T: Component,
    D: Deref<Target = MaskedStorage<T>>,
{
    storage
        .get(entity)
        .ok_or(Error::EntityMissingComponent(entity, unsafe {
            type_name::<T>()
        }))
}

pub fn try_component_mut<'a, T, D>(
    storage: &'a mut Storage<T, D>,
    entity: Entity,
) -> Result<&'a mut T, Error>
where
    T: Component,
    D: DerefMut<Target = MaskedStorage<T>>,
{
    storage
        .get_mut(entity)
        .ok_or(Error::EntityMissingComponent(entity, unsafe {
            type_name::<T>()
        }))
}

pub fn try_join_get<'a, J: Join>(
    entity: Entity,
    entities: &Entities<'a>,
    mut join_iter: JoinIter<J>,
) -> Result<J::Type, Error> {
    join_iter
        .get(entity, entities)
        .ok_or(Error::EntityMissingComponent(entity, unsafe {
            type_name::<J::Type>()
        }))
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
    InvalidState(String),
    InvalidEntity(EntityId),
    MissingComponent(EntityId, &'static str),
    EntityMissingComponent(Entity, &'static str),
}

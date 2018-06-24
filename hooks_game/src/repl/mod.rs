#[macro_use]
pub mod snapshot;
pub mod entity;
pub mod interp;
pub mod player;
pub mod tick;

pub mod component;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::intrinsics::type_name;
use std::ops::{Deref, DerefMut};

use bit_manager::data::BitStore;

use specs;
use specs::join::JoinIter;
use specs::prelude::{Entities, Entity, Join, Storage, VecStorage, World};
use specs::storage::MaskedStorage;

use defs::{EntityClassId, PlayerId};
use registry::Registry;

pub fn register(reg: &mut Registry) {
    component::register(reg);
    entity::register(reg);
    player::register(reg);
}

pub fn try<'a, T, D>(storage: &'a Storage<T, D>, entity: Entity) -> Result<&'a T, Error>
where
    T: specs::prelude::Component,
    D: Deref<Target = MaskedStorage<T>>,
{
    storage
        .get(entity)
        .ok_or_else(|| Error::EntityMissingComponent(entity, unsafe { type_name::<T>() }))
}

pub fn try_mut<'a, T, D>(storage: &'a mut Storage<T, D>, entity: Entity) -> Result<&'a mut T, Error>
where
    T: specs::prelude::Component,
    D: DerefMut<Target = MaskedStorage<T>>,
{
    storage
        .get_mut(entity)
        .ok_or_else(|| Error::EntityMissingComponent(entity, unsafe { type_name::<T>() }))
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

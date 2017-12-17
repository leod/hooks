mod snapshot;

use std::collections::HashMap;

use specs::{Entity, VecStorage};

use defs::{EntityId, EntityKindId, PlayerId};

pub trait ReplComponent {
    const OWNER_ONLY: bool = false;
}

#[derive(Component)]
#[component(VecStorage)]
pub struct ReplId(EntityId);

#[derive(Component)]
#[component(VecStorage)]
pub struct ReplEntity {
    pub owner: PlayerId,
    pub kind: EntityKindId,
}

// Map from shared EntityId to the local Entity
pub struct ReplEntities {
    pub map: HashMap<EntityId, Entity>,
}

impl ReplEntities {
    pub fn id_to_entity(&self, id: EntityId) -> Entity {
        *self.map.get(&id).unwrap() 
    }
}

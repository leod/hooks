mod snapshot;

use std::collections::HashMap;

use specs::{Entity, VecStorage};

use defs::{EntityId};

#[derive(Component)]
#[component(VecStorage)]
pub struct ReplId(EntityId);

// Map from shared EntityId to the local Entity
pub struct ReplEntities {
    pub map: HashMap<EntityId, Entity>,
}

impl ReplEntities {
    pub fn id_to_entity(&self, id: EntityId) -> Entity {
        *self.map.get(&id).unwrap() 
    }
}

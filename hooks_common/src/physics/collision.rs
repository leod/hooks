use specs::{Entities, Entity, FetchMut, HashMapStorage, Join, ReadStorage, System, VecStorage,
            WriteStorage};

use nalgebra::Isometry2;
use ncollide::world::{CollisionGroups, CollisionWorld2, GeometricQueryType};

use super::{CollisionShape, Orientation, Position};

// Tag components
#[derive(Component)]
#[component(HashMapStorage)]
pub struct CreateCollisionObject {
    pub collision_groups: CollisionGroups,
    pub query_type: GeometricQueryType<f32>,
}

#[derive(Component)]
#[component(HashMapStorage)]
pub struct RemoveCollisionObject;

// Handle of a ncollide CollisionObject
#[derive(Component)]
#[component(VecStorage)]
pub struct CollisionObjectUid(usize);

// System for creating collision objects for entities tagged with CreateCollisionObject
pub struct CreateCollisionObjectSys {
    next_uid: usize,
}

impl CreateCollisionObjectSys {
    pub fn new() -> Self {
        Self { next_uid: 0 }
    }
}

type CollisionWorld = CollisionWorld2<f32, Entity>;

impl<'a> System<'a> for CreateCollisionObjectSys {
    type SystemData = (
        FetchMut<'a, CollisionWorld>,
        Entities<'a>,
        ReadStorage<'a, Position>,
        ReadStorage<'a, Orientation>,
        ReadStorage<'a, CollisionShape>,
        WriteStorage<'a, CreateCollisionObject>,
        WriteStorage<'a, CollisionObjectUid>,
    );

    fn run(
        &mut self,
        (
            mut collision_world,
            entities,
            position,
            orientation,
            shape,
            mut create_object,
            mut object_uid,
        ): Self::SystemData,
    ) {
        let created_entities = (&*entities, &position, &orientation, &shape, &create_object)
            .join()
            .map(|(entity, position, orientation, shape, create_object)| {
                let uid = self.next_uid;
                self.next_uid += 1;

                let isometry = Isometry2::new(position.pos.coords, orientation.angle);
                collision_world.deferred_add(
                    uid,
                    isometry,
                    shape.shape.clone(),
                    create_object.collision_groups,
                    create_object.query_type,
                    entity,
                );

                object_uid.insert(entity, CollisionObjectUid(uid));

                entity
            })
            .collect::<Vec<_>>();

        for entity in created_entities {
            create_object.remove(entity);
        }

        for (entity, _) in (&*entities, &create_object).join() {
            panic!(
                "Entity {:?} has CreateCollisionObject but not Position, Orientation or Shape",
                entity
            );
        }
    }
}

// System for removing collision objects for entities tagged with RemoveCollisionObject
struct RemoveCollisionObjectSys;

impl<'a> System<'a> for RemoveCollisionObjectSys {
    type SystemData = (
        FetchMut<'a, CollisionWorld>,
        Entities<'a>,
        WriteStorage<'a, RemoveCollisionObject>,
        WriteStorage<'a, CollisionObjectUid>,
    );

    fn run(
        &mut self,
        (mut collision_world, entities, mut remove_object, mut object_uid): Self::SystemData,
    ) {
        let removed_entities = (&*entities, &mut remove_object, &mut object_uid)
            .join()
            .map(|(entity, _, object_uid)| {
                collision_world.deferred_remove(object_uid.0);
                entity
            })
            .collect::<Vec<_>>();

        for entity in removed_entities {
            remove_object.remove(entity);
            object_uid.remove(entity);
        }

        for (entity, _) in (&*entities, &remove_object).join() {
            panic!(
                "Entity {:?} has RemoveCollisionObject but not CollisionObjectUid",
                entity
            );
        }
    }
}

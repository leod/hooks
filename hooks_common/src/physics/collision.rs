use specs::{BTreeStorage, Entities, Entity, FetchMut, Join, NullStorage, ReadStorage, System,
            VecStorage, WriteStorage};

use nalgebra::Isometry2;
use ncollide::shape::ShapeHandle2;
use ncollide::world::CollisionWorld2;

use physics::{Orientation, Position};
use registry::Registry;

pub use ncollide::shape::{Cuboid, ShapeHandle};
pub use ncollide::world::{CollisionGroups, GeometricQueryType};

pub fn register(reg: &mut Registry) {
    reg.component::<Shape>();
    reg.component::<CreateObject>();
    reg.component::<RemoveObject>();
    reg.component::<ObjectUid>();

    reg.resource(CollisionWorld2::<f32, Entity>::new(0.02, false));
}

type CollisionWorld = CollisionWorld2<f32, Entity>;

// Collision groups. Put them here for now.
pub const GROUP_WALL: usize = 0;
pub const GROUP_PLAYER: usize = 1;

#[derive(Clone, Component)]
#[component(VecStorage)]
pub struct Shape {
    pub shape: ShapeHandle2<f32>,
}

// Tag components
#[derive(Component)]
#[component(BTreeStorage)]
pub struct CreateObject {
    pub groups: CollisionGroups,
    pub query_type: GeometricQueryType<f32>,
}

// TODO: Let's make `RemoveEntity` a thing independent of collision, and allow registering systems
//       to handle the removal of entitites. Alternatively... we could use local events for this
//       somehow? But then again, this would mean that entity removal would not be handled in
//       batches.
#[derive(Component, Default)]
#[component(NullStorage)]
pub struct RemoveObject;

// Handle of a ncollide CollisionObject
#[derive(Component)]
#[component(VecStorage)]
pub struct ObjectUid(usize);

// System for creating collision objects for entities tagged with CreateObject
pub struct CreateObjectSys {
    next_uid: usize,
}

impl CreateObjectSys {
    pub fn new() -> Self {
        Self { next_uid: 0 }
    }
}

impl<'a> System<'a> for CreateObjectSys {
    type SystemData = (
        FetchMut<'a, CollisionWorld>,
        Entities<'a>,
        ReadStorage<'a, Position>,
        ReadStorage<'a, Orientation>,
        ReadStorage<'a, Shape>,
        WriteStorage<'a, CreateObject>,
        WriteStorage<'a, ObjectUid>,
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
                    create_object.groups,
                    create_object.query_type,
                    entity,
                );

                object_uid.insert(entity, ObjectUid(uid));

                entity
            })
            .collect::<Vec<_>>();

        for entity in created_entities {
            create_object.remove(entity);
        }

        for (entity, _) in (&*entities, &create_object).join() {
            panic!(
                "Entity {:?} has CreateObject but not Position, Orientation or Shape",
                entity
            );
        }
    }
}

// System for removing collision objects for entities tagged with RemoveObject
struct RemoveObjectSys;

impl<'a> System<'a> for RemoveObjectSys {
    type SystemData = (
        FetchMut<'a, CollisionWorld>,
        Entities<'a>,
        WriteStorage<'a, RemoveObject>,
        WriteStorage<'a, ObjectUid>,
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
            panic!("Entity {:?} has RemoveObject but not ObjectUid", entity);
        }
    }
}

use specs::{Entities, Entity, FetchMut, Join, ReadStorage, System, WriteStorage};

use nalgebra::Isometry2;
use ncollide::shape::ShapeHandle2;
use ncollide::world::{CollisionObjectHandle, CollisionWorld2};

use physics::{Orientation, Position};
use registry::Registry;
use entity;

pub use ncollide::shape::{Cuboid, ShapeHandle};
pub use ncollide::world::{CollisionGroups, GeometricQueryType};

pub fn register(reg: &mut Registry) {
    reg.component::<Shape>();
    reg.component::<CreateObject>();
    reg.component::<RemoveObject>();
    reg.component::<ObjectHandle>();

    let collision_world = CollisionWorld2::<f32, Entity>::new(0.02);
    reg.resource(collision_world);

    reg.removal_system(RemovalSys, "collision");
}

pub type CollisionWorld = CollisionWorld2<f32, Entity>;

// Collision groups. Put them here for now.
pub const GROUP_WALL: usize = 0;
pub const GROUP_PLAYER: usize = 1;

/// Collision shape.
/// For now, we assume that an object's shape will not change in its lifetime.
#[derive(Clone, Component)]
#[component(VecStorage)]
pub struct Shape(pub ShapeHandle2<f32>);

/// Tag component which indicates that we should inform the collision world of this entity. The
/// component is removed from the entity after that.
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
/// Tag component which indicates that we should remove the collision object.
#[derive(Component, Default)]
#[component(NullStorage)]
pub struct RemoveObject;

/// Handle of an ncollide CollisionObject.
#[derive(Component)]
#[component(VecStorage)]
pub struct ObjectHandle(CollisionObjectHandle);

/// System for running the collision pipeline.
pub struct UpdateSys;

impl<'a> System<'a> for UpdateSys {
    type SystemData = (
        FetchMut<'a, CollisionWorld>,
        WriteStorage<'a, Position>,
        WriteStorage<'a, Orientation>,
        ReadStorage<'a, ObjectHandle>,
    );

    fn run(
        &mut self,
        (mut collision_world, mut position, mut orientation, object_handle): Self::SystemData,
    ) {
        // Update isometry of entities that have moved or rotated
        {
            let position_changed = position.open().1.open().0;
            let orientation_changed = orientation.open().1.open().0;
            let changed = position_changed | orientation_changed;

            for (_, position, orientation, object_handle) in
                (&changed, &position, &orientation, &object_handle).join()
            {
                /*if collision_world.collision_object(object_handle.0).is_none() {
                    // This should happen exactly once for each object when it is first created.
                    // `CreateObjectSys` has added the object, but the collision world has
                    // not been updated yet, so changing the position here would be an error.
                    continue;
                }*/

                let isometry = Isometry2::new(position.0.coords, orientation.0);
                collision_world.set_position(object_handle.0, isometry);
            }
        }

        (&mut position).open().1.clear_flags();
        (&mut orientation).open().1.clear_flags();

        collision_world.update();
    }
}

/// System for creating collision objects for entities tagged with `CreateObject`.
pub struct CreateObjectSys;

impl<'a> System<'a> for CreateObjectSys {
    type SystemData = (
        FetchMut<'a, CollisionWorld>,
        Entities<'a>,
        ReadStorage<'a, Position>,
        ReadStorage<'a, Orientation>,
        ReadStorage<'a, Shape>,
        WriteStorage<'a, CreateObject>,
        WriteStorage<'a, ObjectHandle>,
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
            mut object_handle,
        ): Self::SystemData,
    ) {
        let created_entities = (&*entities, &position, &orientation, &shape, &create_object)
            .join()
            .map(|(entity, position, orientation, shape, create_object)| {
                let isometry = Isometry2::new(position.0.coords, orientation.0);
                let handle = collision_world.add(
                    isometry,
                    shape.0.clone(),
                    create_object.groups,
                    create_object.query_type,
                    entity,
                );

                object_handle.insert(entity, ObjectHandle(handle));

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

/// System for removing collision objects for entities tagged with `RemoveObject`.
pub struct RemoveObjectSys;

impl<'a> System<'a> for RemoveObjectSys {
    type SystemData = (
        Entities<'a>,
        FetchMut<'a, CollisionWorld>,
        WriteStorage<'a, RemoveObject>,
        WriteStorage<'a, ObjectHandle>,
    );

    fn run(
        &mut self,
        (entities, mut collision_world, mut remove_object, mut object_handle): Self::SystemData,
    ) {
        let removed_entities = (&*entities, &mut remove_object, &mut object_handle)
            .join()
            .map(|(entity, _, object_handle)| {
                collision_world.remove(&[object_handle.0]);
                entity
            })
            .collect::<Vec<_>>();

        for entity in removed_entities {
            remove_object.remove(entity);
            object_handle.remove(entity);
        }

        for (entity, _) in (&*entities, &remove_object).join() {
            panic!("Entity {:?} has RemoveObject but no ObjectHandle", entity);
        }
    }
}

/// System for removing collision objects for entities tagged with `entity::Remove`.
struct RemovalSys;

impl<'a> System<'a> for RemovalSys {
    type SystemData = (
        Entities<'a>,
        FetchMut<'a, CollisionWorld>,
        ReadStorage<'a, entity::Remove>,
        WriteStorage<'a, ObjectHandle>,
    );

    fn run(
        &mut self,
        (entities, mut collision_world, remove, mut object_handle): Self::SystemData,
    ) {
        let removed_entities = (&*entities, &remove, &mut object_handle)
            .join()
            .map(|(entity, _, object_handle)| {
                collision_world.remove(&[object_handle.0]);
                entity
            })
            .collect::<Vec<_>>();

        for entity in removed_entities {
            object_handle.remove(entity);
        }
    }
}

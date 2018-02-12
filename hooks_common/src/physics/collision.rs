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
    reg.component::<Object>();
    reg.component::<ObjectHandle>();

    let collision_world = CollisionWorld2::<f32, Entity>::new(0.02);
    reg.resource(collision_world);

    reg.removal_system(RemovalSys, "collision");
}

pub type CollisionWorld = CollisionWorld2<f32, Entity>;

// Collision groups. Put them here for now.
pub const GROUP_WALL: usize = 0;
pub const GROUP_PLAYER: usize = 1;
pub const GROUP_PLAYER_ENTITY: usize = 2;

/// Collision shape.
/// For now, we assume that an object's shape will not change in its lifetime.
#[derive(Clone, Component)]
#[component(VecStorage)]
pub struct Shape(pub ShapeHandle2<f32>);

/// Component which indicates that we should inform the collision world of this entity.
/// Note that only `entity::Active` entities are kept in the collision world.
#[derive(Component)]
#[component(BTreeStorage)]
pub struct Object {
    pub groups: CollisionGroups,
    pub query_type: GeometricQueryType<f32>,
}

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

/// System for making sure that exactly the entities with `entity::Active` and `Object` are present
/// in the collision world. `Position`, `Orientation` and `Shape` also need to be given.
pub struct MaintainSys;

impl<'a> System<'a> for MaintainSys {
    type SystemData = (
        FetchMut<'a, CollisionWorld>,
        Entities<'a>,
        ReadStorage<'a, entity::Active>,
        ReadStorage<'a, Position>,
        ReadStorage<'a, Orientation>,
        ReadStorage<'a, Shape>,
        ReadStorage<'a, Object>,
        WriteStorage<'a, ObjectHandle>,
    );

    fn run(
        &mut self,
        (
            mut collision_world,
            entities,
            active,
            position,
            orientation,
            shape,
            object,
            mut object_handle,
        ): Self::SystemData,
    ) {
        // Create newly active entities in collision world
        let new_handles = (
            &*entities,
            &active,
            &position,
            &orientation,
            &shape,
            &object,
            !&object_handle,
        ).join()
            .filter_map(
                |(entity, active, position, orientation, shape, object, _)| {
                    if active.0 {
                        let isometry = Isometry2::new(position.0.coords, orientation.0);
                        let handle = collision_world.add(
                            isometry,
                            shape.0.clone(),
                            object.groups,
                            object.query_type,
                            entity,
                        );

                        Some((entity, handle))
                    } else {
                        None
                    }
                },
            )
            .collect::<Vec<_>>();

        for &(entity, handle) in &new_handles {
            object_handle.insert(entity, ObjectHandle(handle));
        }

        for (entity, active, _, _) in (&*entities, &active, &object, !&object_handle).join() {
            if active.0 {
                panic!(
                    "Entity {:?} has collision::Object but not Position, Orientation or Shape",
                    entity
                );
            }
        }

        // Remove newly inactive entities from collision world
        let removed_handles = (&*entities, &active, &object_handle)
            .join()
            .filter_map(|(entity, active, object_handle)| {
                if !active.0 {
                    collision_world.remove(&[object_handle.0]);
                    Some(entity)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        for entity in removed_handles {
            object_handle.remove(entity);
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

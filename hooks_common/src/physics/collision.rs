use std::marker::PhantomData;

use specs::prelude::*;
use specs::storage::{BTreeStorage, VecStorage};

use nalgebra::{self, Isometry2};
use ncollide::math::{Isometry, Point};
use ncollide::shape::{self, Ball, Plane, ShapeHandle2};
use ncollide::query::algorithms::{JohnsonSimplex, VoronoiSimplex2, VoronoiSimplex3};
use ncollide::narrow_phase::{BallBallContactGenerator, CompositeShapeShapeContactGenerator,
                             ContactAlgorithm, ContactDispatcher, DefaultNarrowPhase,
                             DefaultProximityDispatcher, OneShotContactManifoldGenerator,
                             PlaneSupportMapContactGenerator, ShapeCompositeShapeContactGenerator,
                             SupportMapPlaneContactGenerator, SupportMapSupportMapContactGenerator};
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

    let contact_dispatcher = StatelessContactDispatcher::new();
    let proximity_dispatcher = DefaultProximityDispatcher::new();
    let narrow_phase =
        DefaultNarrowPhase::new(Box::new(contact_dispatcher), Box::new(proximity_dispatcher));
    let mut collision_world = CollisionWorld2::<f32, Entity>::new(0.02);
    collision_world.set_narrow_phase(Box::new(narrow_phase));
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
#[storage(VecStorage)]
pub struct Shape(pub ShapeHandle2<f32>);

/// Component which indicates that we should inform the collision world of this entity.
/// Note that only `entity::Active` entities are kept in the collision world.
#[derive(Component)]
#[storage(BTreeStorage)]
pub struct Object {
    pub groups: CollisionGroups,
    pub query_type: GeometricQueryType<f32>,
}

/// Handle of an ncollide CollisionObject.
#[derive(Component)]
#[storage(VecStorage)]
pub struct ObjectHandle(CollisionObjectHandle);

/// System for running the collision pipeline.
pub struct UpdateSys {
    modified_position_id: ReaderId<ModifiedFlag>,
    modified_position: BitSet,
    modified_orientation_id: ReaderId<ModifiedFlag>,
    modified_orientation: BitSet,
}

impl UpdateSys {
    pub fn new(world: &mut World) -> UpdateSys {
        let mut position = world.write::<Position>();
        let modified_position_id = position.track_modified();

        let mut orientation = world.write::<Orientation>();
        let modified_orientation_id = orientation.track_modified();

        UpdateSys {
            modified_position_id,
            modified_position: BitSet::new(),
            modified_orientation_id,
            modified_orientation: BitSet::new(),
        }
    }
}

impl<'a> System<'a> for UpdateSys {
    type SystemData = (
        FetchMut<'a, CollisionWorld>,
        ReadStorage<'a, Position>,
        ReadStorage<'a, Orientation>,
        ReadStorage<'a, ObjectHandle>,
    );

    fn run(
        &mut self,
        (mut collision_world, position, orientation, object_handle): Self::SystemData,
    ) {
        // Update isometry of entities that have moved or rotated
        position.populate_modified(&mut self.modified_position_id, &mut self.modified_position);
        orientation.populate_modified(
            &mut self.modified_orientation_id,
            &mut self.modified_orientation,
        );

        {
            let modified = &self.modified_position | &self.modified_orientation;

            for (_, position, orientation, object_handle) in
                (&modified, &position, &orientation, &object_handle).join()
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
            .map(
                |(entity, _active, position, orientation, shape, object, _)| {
                    let isometry = Isometry2::new(position.0.coords, orientation.0);
                    let handle = collision_world.add(
                        isometry,
                        shape.0.clone(),
                        object.groups,
                        object.query_type,
                        entity,
                    );

                    (entity, handle)
                },
            )
            .collect::<Vec<_>>();

        for &(entity, handle) in &new_handles {
            object_handle.insert(entity, ObjectHandle(handle));
        }

        for (entity, _active, _, _) in (&*entities, &active, &object, !&object_handle).join() {
            panic!(
                "Entity {:?} has collision::Object but not Position, Orientation or Shape",
                entity
            );
        }

        // Remove newly inactive entities from collision world
        let removed_handles = (&*entities, !&active, &object_handle)
            .join()
            .map(|(entity, _, object_handle)| {
                collision_world.remove(&[object_handle.0]);
                entity
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

/// `ncollide` contact dispatcher that does not use state from previous tick.
/// Adapted from the `DefaultContactDispatcher`.
/// See also <http://users.nphysics.org/t/using-ncollide-in-less-stateful-ways/163>.
pub struct StatelessContactDispatcher<P: Point, M> {
    _point_type: PhantomData<P>,
    _matrix_type: PhantomData<M>,
}

impl<P: Point, M> StatelessContactDispatcher<P, M> {
    /// Creates a new basic collision dispatcher.
    pub fn new() -> StatelessContactDispatcher<P, M> {
        StatelessContactDispatcher {
            _point_type: PhantomData,
            _matrix_type: PhantomData,
        }
    }
}

impl<P: Point, M: Isometry<P>> ContactDispatcher<P, M> for StatelessContactDispatcher<P, M> {
    fn get_contact_algorithm(
        &self,
        a: &shape::Shape<P, M>,
        b: &shape::Shape<P, M>,
    ) -> Option<ContactAlgorithm<P, M>> {
        let a_is_ball = a.is_shape::<Ball<P::Real>>();
        let b_is_ball = b.is_shape::<Ball<P::Real>>();

        if a_is_ball && b_is_ball {
            Some(Box::new(BallBallContactGenerator::<P, M>::new()))
        } else if a.is_shape::<Plane<P::Vector>>() && b.is_support_map() {
            let wo_manifold = PlaneSupportMapContactGenerator::<P, M>::new();

            if !b_is_ball {
                let mut manifold = OneShotContactManifoldGenerator::new(wo_manifold);
                manifold.set_always_one_shot(true);
                Some(Box::new(manifold))
            } else {
                Some(Box::new(wo_manifold))
            }
        } else if b.is_shape::<Plane<P::Vector>>() && a.is_support_map() {
            let wo_manifold = SupportMapPlaneContactGenerator::<P, M>::new();

            if !a_is_ball {
                let mut manifold = OneShotContactManifoldGenerator::new(wo_manifold);
                manifold.set_always_one_shot(true);
                Some(Box::new(manifold))
            } else {
                Some(Box::new(wo_manifold))
            }
        } else if a.is_support_map() && b.is_support_map() {
            match nalgebra::dimension::<P::Vector>() {
                2 => {
                    let simplex = VoronoiSimplex2::new();
                    let wo_manifold = SupportMapSupportMapContactGenerator::new(simplex);

                    if !a_is_ball && !b_is_ball {
                        let mut manifold = OneShotContactManifoldGenerator::new(wo_manifold);
                        manifold.set_always_one_shot(true);
                        Some(Box::new(manifold))
                    } else {
                        Some(Box::new(wo_manifold))
                    }
                }
                3 => {
                    let simplex = VoronoiSimplex3::new();
                    let wo_manifold = SupportMapSupportMapContactGenerator::new(simplex);

                    if !a_is_ball && !b_is_ball {
                        let mut manifold = OneShotContactManifoldGenerator::new(wo_manifold);
                        manifold.set_always_one_shot(true);
                        Some(Box::new(manifold))
                    } else {
                        Some(Box::new(wo_manifold))
                    }
                }
                _ => {
                    let simplex = JohnsonSimplex::new_w_tls();
                    let wo_manifold = SupportMapSupportMapContactGenerator::new(simplex);

                    if false {
                        // !a_is_ball && !b_is_ball {
                        let mut manifold = OneShotContactManifoldGenerator::new(wo_manifold);
                        manifold.set_always_one_shot(true);
                        Some(Box::new(manifold))
                    } else {
                        Some(Box::new(wo_manifold))
                    }
                }
            }
        } else if a.is_composite_shape() {
            Some(Box::new(CompositeShapeShapeContactGenerator::<P, M>::new()))
        } else if b.is_composite_shape() {
            Some(Box::new(ShapeCompositeShapeContactGenerator::<P, M>::new()))
        } else {
            None
        }
    }
}

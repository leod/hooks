use specs::{Entity, Entities, VecStorage, HashMapStorage, System, ReadStorage, WriteStorage, FetchMut, Join};

use nalgebra::{Point2, Isometry2};
use ncollide::world::{CollisionGroups, CollisionWorld2, GeometricQueryType};
use ncollide::shape::ShapeHandle2;

// Signal components
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

#[derive(Component)]
#[component(VecStorage)]
pub struct CollisionShape {
    pub shape: ShapeHandle2<f32>,
}

#[derive(Component)]
#[component(VecStorage)]
pub struct Position {
    pub pos: Point2<f32>,
}

#[derive(Component)]
#[component(VecStorage)]
pub struct Orientation {
    pub angle: f32,
}

pub struct CreateCollisionObjectSys {
    next_uid: usize
}

impl CreateCollisionObjectSys {
    pub fn new() -> Self {
        Self { 
            next_uid: 0
        }
    }
}

type CollisionWorld = CollisionWorld2<f32, Entity>;

impl<'a> System<'a> for CreateCollisionObjectSys {
    type SystemData = (FetchMut<'a, CollisionWorld>,
                       Entities<'a>,
                       ReadStorage<'a, Position>,
                       ReadStorage<'a, Orientation>,
                       ReadStorage<'a, CollisionShape>,
                       WriteStorage<'a, CreateCollisionObject>,
                       WriteStorage<'a, CollisionObjectUid>);

    fn run(&mut self, (mut collision_world, entities, position, orientation, shape, mut create_object, mut object_uid): Self::SystemData) {
        for (entity, position, orientation, shape, create_object) in (&*entities, &position, &orientation, &shape, &mut create_object).join() {
            let uid = self.next_uid;
            self.next_uid += 1;

            object_uid.insert(entity, CollisionObjectUid(uid));

            let isometry = Isometry2::new(position.pos.coords, 
                                          orientation.angle);
            collision_world.deferred_add(uid, 
                                         isometry,
                                         shape.shape.clone(),
                                         create_object.collision_groups,
                                         create_object.query_type,
                                         entity);
        }
    }
}

struct RemoveCollisionObjectSys;

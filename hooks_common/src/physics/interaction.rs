use std::collections::BTreeMap;

use nalgebra::Point2;
use specs::{Entity, World};

use hooks_util::ordered_pair::OrderedPair;

use defs::EntityClassId;
use registry::Registry;
use entity;

pub fn register(reg: &mut Registry) {
    reg.resource(HandlersSetup(Vec::new()));
    reg.resource(Handlers(BTreeMap::new()));
}

type Handler = fn(&World, Entity, Entity, Point2<f32>);

/// In a module's `register` function, it can happen that another entity class that it wants to
/// interact with has not been registered yet. Thus, we store class names while registering, and
/// only resolve to ids later in `setup`.
struct HandlersSetup(Vec<(String, String, Handler)>);

struct Handlers(BTreeMap<OrderedPair<EntityClassId>, Vec<(EntityClassId, EntityClassId, Handler)>>);

pub fn add(reg: &mut Registry, entity_class_a: &str, entity_class_b: &str, handler: Handler) {
    let mut setup = reg.world().write_resource::<HandlersSetup>();
    setup.0.push((
        entity_class_a.to_string(),
        entity_class_b.to_string(),
        handler,
    ));
}

/// Create `Handlers` from `HandlersSetup` by mapping class names to ids. This should have an
/// effect only once at the start of the game.
fn setup(world: &World) {
    let mut setup = world.write_resource::<HandlersSetup>();
    let mut handlers = world.write_resource::<Handlers>();

    for &(ref entity_class_a, ref entity_class_b, handler) in &setup.0 {
        let id_a = entity::get_class_id(world, entity_class_a).unwrap();
        let id_b = entity::get_class_id(world, entity_class_b).unwrap();
        let id_pair = OrderedPair::new(id_a, id_b);

        let list = handlers.0.entry(id_pair).or_insert(Vec::new());
        list.push((id_a, id_b, handler));
    }

    setup.0.clear();
}

pub fn run(world: &World, entity_a: Entity, entity_b: Entity, pos: Point2<f32>) {
    setup(world);

    let (id_a, id_b) = {
        let meta = world.read::<entity::Meta>();

        // We assume here that every entity has an `entity::Meta` component, i.e. that it was
        // constructed by `entity::create`.
        (
            meta.get(entity_a).unwrap().class_id,
            meta.get(entity_b).unwrap().class_id,
        )
    };
    let id_pair = OrderedPair::new(id_a, id_b);

    debug!("{:?} with {:?}", id_a, id_b);

    let handlers = world.read_resource::<Handlers>().0.get(&id_pair).cloned();

    if let Some(handlers) = handlers {
        for &(handler_id_a, handler_id_b, handler) in &handlers {
            assert!(id_pair == OrderedPair::new(handler_id_a, handler_id_b));

            // Make sure to pass the entities in the order in which the handler expects them
            if id_a == handler_id_a {
                handler(world, entity_a, entity_b, pos);
            } else {
                handler(world, entity_b, entity_a, pos);
            }
        }
    }
}

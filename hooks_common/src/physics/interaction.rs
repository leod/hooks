use std::collections::BTreeMap;

use nalgebra::Point2;
use specs::{Entity, World};

use hooks_util::ordered_pair::OrderedPair;

use defs::EntityClassId;
use registry::Registry;
use entity;

pub fn register(reg: &mut Registry) {
    reg.resource(Handlers(BTreeMap::new()));
}

type Handler = fn(&mut World, Entity, Entity, Point2<f32>);

struct Handlers(BTreeMap<OrderedPair<EntityClassId>, Vec<(EntityClassId, EntityClassId, Handler)>>);

pub fn set<F>(reg: &mut Registry, entity_class_a: &str, entity_class_b: &str, handler: Handler) {
    let id_a = entity::get_class_id(reg.world(), entity_class_a).unwrap();
    let id_b = entity::get_class_id(reg.world(), entity_class_b).unwrap();
    let id_pair = OrderedPair::new(id_a, id_b);

    let mut handlers = reg.world().write_resource::<Handlers>();

    let list = handlers.0.entry(id_pair).or_insert(Vec::new());
    list.push((id_a, id_b, handler));
}

pub fn run(world: &mut World, entity_a: Entity, entity_b: Entity, pos: Point2<f32>) {
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

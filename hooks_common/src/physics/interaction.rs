use std::ops::Deref;
use std::collections::BTreeMap;

use nalgebra::{Point2, Vector2};
use specs::{Entity, Fetch, MaskedStorage, Storage, World};

use hooks_util::ordered_pair::OrderedPair;

use defs::EntityClassId;
use registry::Registry;
use entity;

pub fn register(reg: &mut Registry) {
    reg.resource(HandlersSetup(Vec::new()));
    reg.resource(Handlers(BTreeMap::new()));
}

type Handler = fn(&World, &EntityInfo, &EntityInfo, Point2<f32>);

/// An action that should be taken when two entities overlap in a physics prediction step.
#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Action {
    /// Prevent these entities from overlapping.
    PreventOverlap { rotate_a: bool, rotate_b: bool },
}

#[derive(Clone)]
struct Def {
    action: Option<Action>,
    handler: Option<Handler>,
}

/// The information that is given for one entity when resolving interactions.
#[derive(Clone, Debug)]
pub struct EntityInfo {
    pub entity: Entity,

    /// Collision position in object coordinates.
    pub pos_object: Point2<f32>,

    /// Velocity of the entity at the time of impact, if the entity has a velocity component.
    pub vel: Option<Vector2<f32>>,
}

/// A collision event between two entities.
#[derive(Clone, Debug)]
pub struct Event {
    pub a: EntityInfo,
    pub b: EntityInfo,

    /// Collision position in world-space coordinates.
    pub pos: Point2<f32>,
}

/// In a module's `register` function, it can happen that another entity class that it wants to
/// interact with has not been registered yet. Thus, we store class names while registering, and
/// only resolve to ids later in `setup`.
struct HandlersSetup(Vec<(String, String, Def)>);

pub struct Handlers(BTreeMap<OrderedPair<EntityClassId>, (EntityClassId, EntityClassId, Def)>);

pub fn set(
    reg: &mut Registry,
    entity_class_a: &str,
    entity_class_b: &str,
    action: Option<Action>,
    handler: Option<Handler>,
) {
    let mut setup = reg.world().write_resource::<HandlersSetup>();
    setup.0.push((
        entity_class_a.to_string(),
        entity_class_b.to_string(),
        Def { action, handler },
    ));
}

/// Create `Handlers` from `HandlersSetup` by mapping class names to ids. This should have an
/// effect only once at the start of the game.
fn setup(world: &World) {
    let mut setup = world.write_resource::<HandlersSetup>();
    let mut handlers = world.write_resource::<Handlers>();

    for (entity_class_a, entity_class_b, def) in setup.0.drain(..) {
        let id_a = entity::get_class_id(world, &entity_class_a).unwrap();
        let id_b = entity::get_class_id(world, &entity_class_b).unwrap();
        let id_pair = OrderedPair::new(id_a, id_b);

        assert!(
            !handlers.0.contains_key(&id_pair),
            format!(
                "interaction between {} and {} was set twice",
                entity_class_a, entity_class_b
            )
        );

        handlers.0.insert(id_pair, (id_a, id_b, def));
    }
}

pub fn get_action<D>(
    handlers: &Fetch<Handlers>,
    meta: &Storage<entity::Meta, D>,
    entity_a: Entity,
    entity_b: Entity,
) -> Option<Action>
where
    D: Deref<Target = MaskedStorage<entity::Meta>>,
{
    // TODO: In a bit of an extreme edge case, we might have an interaction here, but not have
    //       called `setup` yet.

    let id_a = meta.get(entity_a).unwrap().class_id;
    let id_b = meta.get(entity_b).unwrap().class_id;
    let id_pair = OrderedPair::new(id_a, id_b);

    handlers
        .0
        .get(&id_pair)
        .map(|&(handler_id_a, _handler_id_b, ref handler)| {
            if id_a == handler_id_a {
                handler.action.clone()
            } else {
                // Fix up actions so that it is in the order of `entity_a` and `entity_b`
                handler.action.as_ref().map(|action| match action {
                    &Action::PreventOverlap { rotate_a, rotate_b } => Action::PreventOverlap {
                        rotate_a: rotate_b,
                        rotate_b: rotate_a,
                    },
                })
            }
        })
        .and_then(|x| x)
}

pub fn run(world: &World, event: &Event) {
    setup(world);

    let (id_a, id_b) = {
        let meta = world.read::<entity::Meta>();

        // We assume here that every entity has an `entity::Meta` component, i.e. that it was
        // constructed by `entity::create`.
        (
            meta.get(event.a.entity).unwrap().class_id,
            meta.get(event.b.entity).unwrap().class_id,
        )
    };
    let id_pair = OrderedPair::new(id_a, id_b);

    let def = world.read_resource::<Handlers>().0.get(&id_pair).cloned();

    if let Some((handler_id_a, handler_id_b, def)) = def {
        assert!(id_pair == OrderedPair::new(handler_id_a, handler_id_b));

        if let Some(handler) = def.handler {
            // Make sure to pass the entities in the order in which the handler expects them
            if id_a == handler_id_a {
                handler(world, &event.a, &event.b, event.pos);
            } else {
                handler(world, &event.b, &event.a, event.pos);
            }
        }
    }
}

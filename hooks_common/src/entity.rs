use std::collections::BTreeMap;

use specs::{Entity, EntityBuilder, Join, World};

use defs::EntityClassId;
use registry::Registry;

pub fn register(reg: &mut Registry) {
    reg.resource(Ctors(BTreeMap::new()));
    reg.resource(ClassIds(BTreeMap::new()));

    reg.component::<Meta>();
    reg.component::<Remove>();
}

// TODO: Probably want to use Box<FnSomething>
pub type Ctor = fn(EntityBuilder) -> EntityBuilder;

/// Constructors, e.g. for adding client-side-specific components to replicated entities.
struct Ctors(BTreeMap<EntityClassId, Vec<Ctor>>);

/// Maps from entity class names to their unique id. This map should be exactly the same on server
/// and clients and not change during a game.
pub struct ClassIds(pub BTreeMap<String, EntityClassId>);

/// Meta-information about entities.
#[derive(Component, Debug, Clone, PartialEq, BitStore)]
#[component(VecStorage)]
pub struct Meta {
    pub class_id: EntityClassId,
}

/// Entities tagged with this component shall be removed at the end of the tick.
#[derive(Component, Debug)]
#[component(NullStorage)]
pub struct Remove;

pub fn is_class_id_valid(world: &World, class_id: EntityClassId) -> bool {
    world.read_resource::<Ctors>().0.contains_key(&class_id)
}

pub fn get_class_id(world: &World, name: &str) -> Option<EntityClassId> {
    world.read_resource::<ClassIds>().0.get(name).cloned()
}

pub fn register_class(reg: &mut Registry, name: &str, ctor: Ctor) -> EntityClassId {
    let world = reg.world();

    let mut ctors = world.write_resource::<Ctors>();
    let mut class_ids = world.write_resource::<ClassIds>();

    let class_id = ctors.0.keys().next_back().map(|&id| id + 1).unwrap_or(0);

    assert!(!ctors.0.contains_key(&class_id));
    assert!(!class_ids.0.values().any(|&id| id == class_id));

    ctors.0.insert(class_id, vec![ctor]);
    class_ids.0.insert(name.to_string(), class_id);

    assert!(ctors.0.len() == class_ids.0.len());

    class_id
}

pub fn add_ctor(reg: &mut Registry, name: &str, ctor: Ctor) {
    let world = reg.world();

    let class_id = {
        let class_ids = world.read_resource::<ClassIds>();
        class_ids.0[name]
    };

    let mut ctors = world.write_resource::<Ctors>();
    let ctor_vec = ctors.0.get_mut(&class_id).unwrap();
    ctor_vec.push(ctor);
}

pub fn create<F>(world: &mut World, class_id: EntityClassId, ctor: F) -> Entity
where
    F: FnOnce(EntityBuilder) -> EntityBuilder,
{
    let ctors = world.read_resource::<Ctors>().0[&class_id].clone();

    // Build entity
    let builder = world.create_entity().with(Meta { class_id });

    let builder = ctors.iter().fold(builder, |builder, ctor| ctor(builder));

    // Custom constructor
    let builder = ctor(builder);

    builder.build()
}

pub fn deferred_remove(world: &World, entity: Entity) {
    world.write::<Remove>().insert(entity, Remove);
}

pub fn perform_removals(world: &mut World) {
    {
        let entities = world.entities();
        let mut remove = world.write::<Remove>();

        for (entity, _) in (&*entities, &remove).join() {
            entities.delete(entity).unwrap();
        }

        remove.clear();
    }

    world.maintain();
}

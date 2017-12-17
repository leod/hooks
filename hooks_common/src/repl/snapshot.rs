use std::cocollections::BTreeMap;

use serde::ser::{Serialize, Serializer};
use serde::de::{Deserialize, Deserializer};

use defs::{EntityId, PlayerId, INVALID_ENTITY_ID};
use ordered_join;

/// Trait implemented by the EntitySnapshot struct in the `snapshot!` macro.
/// An EntitySnapshot stores the state of a set of components of one entity.
pub trait EntitySnapshot: Serialize + Deserialize {
    /// Returns the state of only the next components that have changed.
    /// This can then be used to delta serialize the snapshot.
    fn delta(&self, next: &Self) -> Self;

    /// Filter out components that are only shared with the entity's owner
    fn filter_by_owner(&mut self, player_id: PlayerId);

    /// Returns true if the snapshot contains the state of at least one component
    fn any(&self) -> bool;
}

/// Entity state snapshots for replicated entities
pub struct WorldSnapshot<T: EntitySnapshot>(pub BTreeMap<EntityId, T>);

impl<T: EntitySnapshot> WorldSnapshot<T> {
    pub fn new() -> Self {
        WorldSnapshot (
            BTreeMap::new()
        )
    }
}

impl<T: Serialize> WorldSnapshot<T> {
    /// Serialize only those entities and components that have changed from this tick to the next one
    fn delta_serialize<S>(&self, next: &Self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_tuple(0);

        let join = ordered_join::FullJoinIter::new(self.0.iter(),
                                                   next.0.iter());
        for join_item in join {
            ItemKind::Both(id, left, right) => {
                assert!(id != INVALID_ENTITY_ID);

                if left.any_delta(right) {
                    seq.serialize_element(id);

                }
            }
        }
    }
}

/// This macro generates an `EntitySnapshot` struct to be able to copy the state of a
/// selection of components from a `specs::World`. We only replicate entities that have a
/// `ReplId` component storing the unique EntityId shared by the server and
/// all clients. 
///
/// The macro requires a list of names and types of the components to be stored.
/// The components are assumed to implement Component, Clone, PartialEq,
/// Serialize and Deserialize.
///
/// We provide the following systems:
/// - StoreSnapshotSys: Store `specs::World` state in a `Snapshot`.
/// - StoreDeltaSnapshotSys: Store only the state that changed compared to
///                          another snapshot.
/// - LoadSnapshotSys: Load state from a `Snapshot` into a `specs::World`.
///
/// All structs are generated in a submodule.
///
/// `Snapshot`s are serializable. This makes it possible to replicate state
/// from the server to clients. By storing multiple sequential `Snapshot`s,
/// the client can smoothly interpolate the received states.
macro_rules! snapshot {
    {
        $(use $use_head:ident$(::$use_tail:ident)*;)*
        mod $name: ident {
            $($field_name:ident: $field_type:ident),+,
        }
    } => {
        pub mod $name {
            use std::fmt;

            use serde::ser::{Serialize, Serializer, SerializeTuple};
            use serde::de::{Deserialize, Deserializer, Visitor, SeqAccess};

            use specs::{Entity, Entities, VecStorage, HashMapStorage, System, ReadStorage, WriteStorage, Fetch, Join, World};

            use defs::{EntityId, INVALID_ENTITY_ID};
            use repl::{ReplEntity, ReplEntities};
            use repl::snapshot;

            $(use $use_head$(::$use_tail)*;)*

            // Complete replicated state of one entity. Note that not every
            // component needs to be given for every entity.
            pub struct EntitySnapshot {
                $(
                    pub $field_name: Option<$field_type>,
                )+
            }

            impl EntitySnapshot {
                pub fn none() -> Self {
                    Self {
                        $(
                            $field_name: None,
                        )+
                    }
                }
            }

            pub enum Component {
                $(
                    $field_type,
                )+
            }

            pub type WorldSnapshot = snapshot::WorldSnapshot<EntitySnapshot>;

            /// Store World state of entities with ReplId component in a Snapshot
            pub struct StoreSnapshotSys<'a>(pub &'a mut WorldSnapshot);

            impl<'a> System<'a> for StoreSnapshotSys<'a> {
                type SystemData = (Entities<'a>,
                                   ReadStorage<'a, ReplEntity>,
                                   $(
                                       ReadStorage<'a, $field_type>,
                                   )+);

                fn run(&mut self, (entities, repl_entity, $($field_name,)+): Self::SystemData) {
                    (self.0).0.clear();

                    for (entity, repl_entity) in (&*entities, &repl_entity).join() {
                        let entity_snapshot = EntitySnapshot {
                            $(
                                $field_name: $field_name.get(entity).map(|c| c.clone()),
                            )+
                        };
                        (self.0).0.push((repl_entity.id, entity_snapshot));
                    }
                }
            }

            /// Overwrite World state of entities with ReplId component with the state in a Snapshot
            pub struct LoadSnapshotSys<'a>(pub &'a WorldSnapshot);

            impl<'a> System<'a> for LoadSnapshotSys<'a> {
                type SystemData = (Fetch<'a, ReplEntities>,
                                   $(
                                       WriteStorage<'a, $field_type>,
                                   )+);

                fn run(&mut self, (repl_entities, $(mut $field_name,)+): Self::SystemData) {
                    for (&entity_id, entity_snapshot) in (self.0).0.iter() {
                        let entity = repl_entities.id_to_entity(entity_id);

                        $(
                            if let Some(component) = entity_snapshot.$field_name.as_ref() {
                                $field_name.insert(entity, component.clone());
                            }
                        )+
                    }
                }
            }

            // Implement EntitySnapshot
            impl snapshot::EntitySnapshot for EntitySnapshot {
                fn delta(&self, next: &Self) -> Self {
                    let mut snapshot = EntitySnapshot::none();

                    $(
                        // See if this component is different in the next snapshot
                        let changed =
                            match (self.$field_name.as_ref(), next.$field_name.as_ref()) {
                                (Some(component), Some(next_component)) =>
                                    component != next_component,
                                (None, Some(next_component)) =>
                                    true,
                                _ => false
                            };

                        if changed {
                            snapshot.$field_name = Some(next.$field_name.unwrap().clone());
                        }
                    )+

                    return snapshot;
                }
 
                fn filter_by_owner(&mut self, player_id: PlayerId) {
                    if self.repl_entity.owner != player_id {
                        $(
                            if $field_type::OWNER_ONLY {
                                self.$field_name = None;
                            }
                        )+
                    }
                }

                fn any(&self) -> bool {
                    $(self.$field_name.is_some() || )+ false
                }
            }

            // Serialize EntitySnapshots
            type ComponentsBitSet = u16; // TODO: This fails for >16 components

            #[allow(unused_assignments)]
            fn components_bit_set(snapshot: &EntitySnapshot) -> ComponentsBitSet {
                let mut bit_set: ComponentsBitSet = 0; 
                let mut i = 0;

                $(
                    if snapshot.$field_name.is_some() {
                        bit_set |= 1 << i;
                    }

                    i += 1;
                )+

                bit_set
            }

            impl Serialize for EntitySnapshot {
                fn serialize<S>(&self, next: &EntitySnapshot, serializer: S) -> Result<S::Ok, S::Error>
                    where S: Serializer
                {
                    // TODO: Here, we could assume that the set of components of one entity does
                    // not change in its lifetime. Then we could use fewer than 16 bits.
                    let mut bit_set = components_bit_set(self);

                    let mut seq = serializer.serialize_tuple(1 + bit_set.count_ones() as usize)?;

                    seq.serialize_element(&bit_set)?;

                    $(
                        if bit_set & 1 == 1 {
                            let component = self.$field_name.as_ref().unwrap();
                            seq.serialize_element(&component)?;
                        }
                        bit_set <<= 1;
                    )+

                    seq.end()
                }
            }

            impl<'de> Deserialize<'de> for EntitySnapshot {
                fn deserialize<'de, D>(&mut self, deserializer: D) -> Result<(), D::Error>
                    where D: Deserializer<'de>
                {
                    struct ComponentsVisitor<'a>(&'a mut EntitySnapshot);

                    impl<'a, 'de> Visitor<'de> for ComponentsVisitor<'a> {
                        type Value = EntitySnapshot;

                        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                            write!(formatter, "Expected components tuple")
                        }

                        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
                            let bit_set = seq.next_element::<ComponentsBitSet>()?.unwrap();

                            $(
                                if bit_set & 1 == 1 {
                                    let component = seq.next_element::<$field_type>()?.unwrap();
                                    (self.0).$field_name = Some(component);
                                }
                            )+

                            Ok(())
                        }
                    }

                    deserializer.deserialize_tuple(0, ComponentsVisitor(self))
                }
            }
        }
    }
}

snapshot! {
    use repl::ReplEntity,
    use physics::Position;
    use physics::Orientation;

    mod net_repl {
        repl_entity: ReplEntity,
        position: Position,
        orientation: Orientation,
    }
}

use specs::{World, RunNow};
pub fn f(x: &mut net_repl::Snapshot, y: &mut World) {
    net_repl::StoreSnapshotSys(x).run_now(&mut y.res);

    let mut snap: net_repl::Snapshot = net_repl::Snapshot::new();

    for item in ordered_join::FullJoinIter::new(x.0.iter_mut(), snap.0.iter()) {
    }
}

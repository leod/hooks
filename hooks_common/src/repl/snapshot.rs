use std::collections::BTreeMap;

use bit_manager::{BitRead, BitWrite, Result};

use super::Entity;
use defs::{EntityClassId, EntityId, PlayerId, INVALID_ENTITY_ID};
use ordered_join;

/// Trait implemented by the EntitySnapshot struct in the `snapshot!` macro. An EntitySnapshot
/// stores the state of a set of components of one entity.
pub trait EntitySnapshot: Clone + PartialEq {
    /// Identifier for types of components possibly held in a snapshot.
    type ComponentType;

    /// Empty entity with no components stored.
    fn none() -> Self;

    /// Write only the components that changed from `self` to `cur`.
    fn delta_write<W: BitWrite>(
        &self,
        cur: &Self,
        components: &[Self::ComponentType],
        writer: &mut W,
    ) -> Result<()>;

    /// Return updated state with changed components as read in the bitstream.
    fn delta_read<R: BitRead>(
        &self,
        components: &[Self::ComponentType],
        reader: &mut R,
    ) -> Result<Self>;
}

/// Meta information about replicated entity types.
pub struct EntityClass<T: EntitySnapshot> {
    /// Which components are to be replicated for this entity type. We use this knowledge to create
    /// a smaller representation of the entity delta snapshot in the bitstreams. This means that
    /// the set of components which are replicated for one entity can not change during its
    /// lifetime.
    pub components: Vec<T::ComponentType>,
}

/// All possible replicated entity types. Every replicated entity has a `repl::Entity` component,
/// storing an index into this map.
pub struct EntityClasses<T: EntitySnapshot>(pub BTreeMap<EntityClassId, EntityClass<T>>);

impl<T: EntitySnapshot> EntityClasses<T> {
    pub fn new() -> Self {
        EntityClasses(BTreeMap::new())
    }
}

/// Snapshot of a set of entities at one point in time. In addition to the EntitySnapshot, we store
/// the entities' meta-information `repl::Entity` here as well, so that we know which components
/// should be replicated.
#[derive(PartialEq)]
pub struct WorldSnapshot<T: EntitySnapshot>(pub BTreeMap<EntityId, (Entity, T)>);

impl<T: EntitySnapshot> WorldSnapshot<T> {
    pub fn new() -> Self {
        WorldSnapshot(BTreeMap::new())
    }
}

impl<T: EntitySnapshot> WorldSnapshot<T> {
    /// Write only those entities and components that have changed compared to a previous tick.
    /// The entities are written ordered by id.
    pub fn delta_write<W: BitWrite>(
        &self,
        cur: &Self,
        classes: &EntityClasses<T>,
        recv_player_id: PlayerId,
        writer: &mut W,
    ) -> Result<()> {
        // Iterate entity pairs contained in the previous (left) and the next (right) snapshot
        for join_item in ordered_join::FullJoinIter::new(self.0.iter(), cur.0.iter()) {
            match join_item {
                ordered_join::Item::Left(&id, _left) => {
                    // The entity stopped existing in the new snapshot - write nothing
                    assert!(id != INVALID_ENTITY_ID);
                }
                ordered_join::Item::Right(&id, &(ref right_entity, ref right_snapshot)) => {
                    // We have a new entity
                    assert!(id != INVALID_ENTITY_ID);

                    writer.write(&id)?;

                    // Write meta-information of the entity
                    writer.write(right_entity)?;

                    // Write all of the components
                    let components = &classes.0.get(&right_entity.class_id).unwrap().components;
                    let left_snapshot = T::none();
                    left_snapshot.delta_write(right_snapshot, components, writer)?;
                }
                ordered_join::Item::Both(
                    &id,
                    &(ref left_entity, ref left_snapshot),
                    &(ref right_entity, ref right_snapshot),
                ) => {
                    // This entity exists in the left and the right snapshot
                    assert!(id != INVALID_ENTITY_ID);
                    assert!(left_entity == right_entity);

                    // We only need to write this entity if at least one component has changed
                    if left_snapshot != right_snapshot {
                        writer.write(&id)?;

                        let components = &classes.0.get(&left_entity.class_id).unwrap().components;

                        // Write all the changed components
                        left_snapshot.delta_write(&right_snapshot, components, writer)?;
                    }
                }
            }
        }

        // The invalid entity id signifies the end of the snapshot.
        // TODO: Figure out if this can be left out by knowing when the BitRead is exhausted.
        writer.write(&INVALID_ENTITY_ID)?;

        Ok(())
    }

    /// Return a new snapshot, updating entities and components from the received delta.
    /// The return type is a tuple, where the first element is a list of new entities and the
    /// second element is the `WorldSnapshot`.
    pub fn delta_read<R: BitRead>(
        &self,
        classes: &EntityClasses<T>,
        reader: &mut R,
    ) -> Result<(Vec<EntityId>, WorldSnapshot<T>)> {
        let mut new_entities = Vec::new();
        let mut cur_snapshot = WorldSnapshot(BTreeMap::new());

        // Iterate entity pairs contained in the previous (left) and the delta (right) snapshot
        let mut prev_entity_iter = self.0.iter().peekable();

        let mut delta_id: Option<EntityId> = None;
        let mut delta_finished = false;

        loop {
            if delta_id.is_none() && !delta_finished {
                // Read next entity id from the delta bitstream
                let id = reader.read()?;

                if id == INVALID_ENTITY_ID {
                    // End of entity stream
                    delta_id = None;
                    delta_finished = true;
                } else {
                    delta_id = Some(id);
                }
            }

            // Join with previous entities
            let left = prev_entity_iter.peek().map(|&(&id, entity)| (id, entity));
            let right = delta_id.map(|id| (id, ()));

            let (left_next, right_next) = match ordered_join::full_join_item(left, right) {
                Some(item) => {
                    match item {
                        ordered_join::Item::Left(id, left) => {
                            // No new information about this entity
                            assert!(id != INVALID_ENTITY_ID);

                            // Keep previous snapshot
                            cur_snapshot.0.insert(id, (*left).clone());
                        }
                        ordered_join::Item::Right(id, _) => {
                            // New entity
                            assert!(id != INVALID_ENTITY_ID);
                            new_entities.push(id);

                            // Read meta-information
                            let entity: Entity = reader.read()?;

                            // Read all components
                            let components = &classes.0.get(&entity.class_id).unwrap().components;
                            let left_snapshot = T::none();
                            let entity_snapshot = left_snapshot.delta_read(components, reader)?;

                            cur_snapshot.0.insert(id, (entity, entity_snapshot));
                        }
                        ordered_join::Item::Both(id, &(ref left_entity, ref left_snapshot), _) => {
                            // This entity exists in both snapshots
                            assert!(id != INVALID_ENTITY_ID);

                            // Update existing entity snapshot with delta from the stream
                            let components =
                                &classes.0.get(&left_entity.class_id).unwrap().components;
                            let entity_snapshot = left_snapshot.delta_read(components, reader)?;

                            cur_snapshot
                                .0
                                .insert(id, (left_entity.clone(), entity_snapshot));
                        }
                    }

                    item.next_flags()
                }
                None => {
                    // Both snapshots are exhausted
                    break;
                }
            };

            // Advance iterators
            if left_next {
                prev_entity_iter.next();
            }
            if right_next {
                // The next delta item will be read on the next iteration of the loop
                delta_id = None;
            }
        }

        Ok((new_entities, cur_snapshot))
    }
}

/// This macro generates an `EntitySnapshot` struct to be able to copy the state of a selection of
/// components from a `specs::World`.
///
/// We only replicate entities that have a `repl::Id` component, which stores the unique EntityId
/// shared by the server and all clients.
///
/// The macro is given a list of names and types of the components to be stored. The components are
/// assumed to implement `Component`, `Clone`, `PartialEq` and `BitStore`.
///
/// The macro generates the following systems to interact between `WorldSnapshot` and
/// `specs::World`:
/// - StoreSnapshotSys: Store `specs::World` state in a `WorldSnapshot`.
/// - LoadSnapshotSys: Load state from a `WorldSnapshot` into a `specs::World`.
///
/// All types are generated in a submodule.
///
/// `WorldSnapshot`s are serializable. This makes it possible to replicate state from the server to
/// clients. By storing multiple sequential `WorldSnapshot`s, the client can smoothly interpolate
/// the received states.
macro_rules! snapshot {
    {
        $(use $use_head:ident$(::$use_tail:ident)*;)*
        mod $name: ident {
            $($field_name:ident: $field_type:ident),+,
        }
    } => {
        pub mod $name {
            use bit_manager::{Result, BitRead, BitWrite};

            use specs::{Entities, System, ReadStorage, WriteStorage, Fetch, Join};

            use repl::{self, snapshot};

            $(use $use_head$(::$use_tail)*;)*

            /// All components that can be replicated with an EntitySnapshot.
            #[derive(Clone, Copy, PartialEq, Debug)]
            pub enum ComponentType {
                $(
                    $field_type,
                )+
            }

            /// Complete replicated state of one entity. Note that not every component needs to be
            /// given for every entity.
            #[derive(Clone, PartialEq)]
            pub struct EntitySnapshot {
                $(
                    pub $field_name: Option<$field_type>,
                )+
            }

            impl snapshot::EntitySnapshot for EntitySnapshot {
                type ComponentType = ComponentType;

                fn none() -> Self {
                    Self {
                        $(
                            $field_name: None,
                        )+
                    }
                }

                fn delta_write<W: BitWrite>(
                    &self,
                    cur: &Self,
                    components: &[Self::ComponentType],
                    writer: &mut W
                ) -> Result<()> {
                    for component in components {
                        match component {
                            $(
                                &ComponentType::$field_type => {
                                    match (self.$field_name.as_ref(), cur.$field_name.as_ref()) {
                                        (Some(left), Some(right)) => {
                                            // Only write the component if it has changed
                                            let changed: bool = left != right;
                                            writer.write_bit(changed)?;
                                            if changed {
                                                writer.write(right)?;
                                            }
                                        }
                                        (None, Some(right)) => {
                                            // This should only happen for new entities.
                                            // TODO: Here we could optimize by not writing the
                                            //       bits at all.
                                            writer.write_bit(true)?;
                                            writer.write(right)?;
                                        }
                                        (None, None) => {
                                            panic!("Trying to write a none component of type {:?}",
                                                   ComponentType::$field_type);
                                        }
                                        _ => {
                                            panic!("Set of replicated components must not change");
                                        }
                                    }
                                }
                            )+
                        }
                    }

                    Ok(())
                }

                fn delta_read<R: BitRead>(
                    &self,
                    components: &[Self::ComponentType],
                    reader: &mut R
                ) -> Result<Self> {
                    let mut result = Self::none();

                    for component in components {
                        match component {
                            $(
                                &ComponentType::$field_type => {
                                    let changed = reader.read_bit()?;

                                    if changed {
                                        // Component has changed, so read the updated value
                                        result.$field_name = Some(reader.read()?);
                                    } else {
                                        // Component has not changed, so take the previous value
                                        assert!(self.$field_name.is_some());
                                        result.$field_name = self.$field_name.clone();
                                    }
                                }
                            )+
                        }
                    }

                    Ok(result)
                }
            }

            pub type EntityClass = snapshot::EntityClass<EntitySnapshot>;
            pub type EntityClasses = snapshot::EntityClasses<EntitySnapshot>;
            pub type WorldSnapshot = snapshot::WorldSnapshot<EntitySnapshot>;

            /// Store World state of entities with ReplId component in a Snapshot.
            pub struct StoreSnapshotSys<'a>(pub &'a mut WorldSnapshot);

            impl<'a> System<'a> for StoreSnapshotSys<'a> {
                type SystemData = (
                    Fetch<'a, EntityClasses>,
                    Entities<'a>,
                    ReadStorage<'a, repl::Id>,
                    ReadStorage<'a, repl::Entity>,
                    $(
                        ReadStorage<'a, $field_type>,
                    )+
                );

                fn run(
                    &mut self,
                    (classes, entities, repl_id, repl_entity, $($field_name,)+): Self::SystemData,
                ) {
                    (self.0).0.clear();

                    let join = (&*entities, &repl_id, &repl_entity).join();
                    for (entity, repl_id, repl_entity) in join {
                        let components = &classes.0.get(&repl_entity.class_id).unwrap().components;

                        let mut entity_snapshot: EntitySnapshot = snapshot::EntitySnapshot::none();
                        for component in components {
                            match component {
                                $(
                                    &ComponentType::$field_type => entity_snapshot.$field_name =
                                        Some($field_name.get(entity).unwrap().clone()),
                                )+
                            }
                        }

                        (self.0).0.insert(repl_id.0, (repl_entity.clone(), entity_snapshot));
                    }
                }
            }

            /// Overwrite World state of entities with `ReplId` component with the state in a
            /// Snapshot. Note that this system does not create new entities.
            pub struct LoadSnapshotSys<'a>(pub &'a WorldSnapshot);

            impl<'a> System<'a> for LoadSnapshotSys<'a> {
                type SystemData = (
                    Fetch<'a, repl::Entities>,
                    $(
                        WriteStorage<'a, $field_type>,
                    )+
                );

                fn run(&mut self, (repl_entities, $(mut $field_name,)+): Self::SystemData) {
                    for (&entity_id, entity_snapshot) in (self.0).0.iter() {
                        let entity = repl_entities.id_to_entity(entity_id);

                        $(
                            if let Some(component) = (entity_snapshot.1).$field_name.as_ref() {
                                $field_name.insert(entity, component.clone());
                            }
                        )+
                    }
                }
            }
        }
    }
}

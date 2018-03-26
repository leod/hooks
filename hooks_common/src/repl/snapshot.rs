use std::collections::BTreeMap;
use std::fmt::Debug;

use bit_manager::{self, BitRead, BitWrite};

use hooks_util::{join, stats};

use defs::{EntityClassId, EntityId, PlayerId, INVALID_ENTITY_ID};
use entity::Meta;
use repl;

#[derive(Debug)]
pub enum Error {
    ReceivedInvalidSnapshot(String),
    BitManager(bit_manager::Error),
}

impl From<bit_manager::Error> for Error {
    fn from(error: bit_manager::Error) -> Error {
        Error::BitManager(error)
    }
}

/// Trait implemented by the EntitySnapshot struct in the `snapshot!` macro. An EntitySnapshot
/// stores the state of a set of components of one entity.
pub trait EntitySnapshot: Clone + PartialEq + 'static {
    /// Identifier for types of components possibly held in a snapshot.
    type ComponentType: ComponentType<EntitySnapshot = Self>;

    /// Empty entity with no components stored.
    fn none() -> Self;

    /// Write only the components that changed from `self` to `cur`.
    fn delta_write<W: BitWrite>(
        &self,
        cur: &Self,
        components: &[Self::ComponentType],
        writer: &mut W,
    ) -> Result<(), bit_manager::Error>;

    /// Return updated state with changed components as read in the bitstream.
    fn delta_read<R: BitRead>(
        &self,
        components: &[Self::ComponentType],
        reader: &mut R,
    ) -> Result<Self, Error>;

    /// Calculate some kind of measure of how different two entity snapshots are.
    fn distance(&self, other: &Self) -> Result<f32, repl::Error>;
}

pub trait HasComponent<T> {
    fn get(&self) -> Option<T>;
}

/// Trait implemented by the component type enum associated with an EntitySnapshot.
pub trait ComponentType: Debug + Clone + Sync + Send + Sized {
    type EntitySnapshot: EntitySnapshot<ComponentType = Self>;
}

/// Meta information about replicated entity types.
pub struct EntityClass<T: EntitySnapshot> {
    /// Which components are to be replicated for this entity type. We use this knowledge to create
    /// a smaller representation of the entity delta snapshot in the bitstreams. This means that
    /// the set of components which are replicated for one entity can not change during its
    /// lifetime.
    pub components: Vec<T::ComponentType>,
}

/// All possible replicated entity types. Every replicated entity has a `entity::Meta` component,
/// storing an index into this map.
pub struct EntityClasses<T: EntitySnapshot>(pub BTreeMap<EntityClassId, EntityClass<T>>);

impl<T: EntitySnapshot> EntityClasses<T> {
    pub fn new() -> Self {
        EntityClasses(BTreeMap::new())
    }
}

/// Snapshot of a set of entities at one point in time. In addition to the EntitySnapshot, we store
/// the entities' meta-information here as well, so that we know which components should be
/// replicated.
#[derive(PartialEq, Clone)]
pub struct WorldSnapshot<T: EntitySnapshot>(pub BTreeMap<EntityId, (Meta, T)>);

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
        _recv_player_id: PlayerId,
        writer: &mut W,
    ) -> Result<(), bit_manager::Error> {
        // Iterate entity pairs contained in the previous (left) and the next (right) snapshot
        for join_item in join::FullJoinIter::new(self.0.iter(), cur.0.iter()) {
            match join_item {
                join::Item::Left(&id, _left) => {
                    // The entity stopped existing in the new snapshot - write nothing
                    assert!(id != INVALID_ENTITY_ID);
                }
                join::Item::Right(&id, &(ref right_meta, ref right_snapshot)) => {
                    // We have a new entity
                    assert!(id != INVALID_ENTITY_ID);

                    writer.write(&id)?;

                    // Write meta-information of the entity
                    writer.write(right_meta)?;

                    // Write all of the components
                    let components = &classes.0[&right_meta.class_id].components;
                    let left_snapshot = T::none();
                    left_snapshot.delta_write(right_snapshot, components, writer)?;
                }
                join::Item::Both(
                    &id,
                    &(ref left_meta, ref left_snapshot),
                    &(ref right_meta, ref right_snapshot),
                ) => {
                    // This entity exists in the left and the right snapshot
                    assert!(id != INVALID_ENTITY_ID);
                    assert!(left_meta == right_meta);

                    // We only need to write this entity if at least one component has changed
                    if left_snapshot != right_snapshot {
                        writer.write(&id)?;

                        let components = &classes.0[&left_meta.class_id].components;

                        // Write all the changed components
                        left_snapshot.delta_write(right_snapshot, components, writer)?;
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
    ) -> Result<(Vec<EntityId>, WorldSnapshot<T>), Error> {
        let mut new_entities = Vec::new();
        let mut cur_snapshot = WorldSnapshot(BTreeMap::new());

        // Iterate entity pairs contained in the previous (left) and the delta (right) snapshot
        let mut prev_entity_iter = self.0.iter().peekable();

        let mut delta_id: Option<EntityId> = None;
        let mut delta_finished = false;

        // Counter for debugging / statistics
        let mut num_entities_read = 0;

        loop {
            if delta_id.is_none() && !delta_finished {
                // Read next entity id from the delta bitstream
                let next_id = reader.read()?;

                if next_id == INVALID_ENTITY_ID {
                    // End of entity stream
                    delta_id = None;
                    delta_finished = true;
                } else {
                    // Ids must be sent in sorted order and without duplicates
                    if let Some(prev_id) = delta_id {
                        if next_id <= prev_id {
                            return Err(Error::ReceivedInvalidSnapshot(
                                "entity ids in snapshot are not sorted".to_string(),
                            ));
                        }
                    }

                    delta_id = Some(next_id);
                }
            }

            // Join with previous entities
            let left = prev_entity_iter.peek().map(|&(&id, entity)| (id, entity));
            let right = delta_id.map(|id| (id, ()));

            let (left_next, right_next) = match join::full_join_item(left, right) {
                Some(item) => {
                    match item {
                        join::Item::Left(id, left) => {
                            // No new information about this entity
                            assert!(id != INVALID_ENTITY_ID);

                            // Keep previous snapshot
                            cur_snapshot.0.insert(id, (*left).clone());
                        }
                        join::Item::Right(id, _) => {
                            num_entities_read += 1;

                            // New entity
                            assert!(id != INVALID_ENTITY_ID);
                            new_entities.push(id);

                            // Read meta-information
                            let meta: Meta = reader.read()?;

                            // Check that we have this class
                            let class = classes.0.get(&meta.class_id);
                            if class.is_none() {
                                return Err(Error::ReceivedInvalidSnapshot(
                                    "invalid class id in entity snapshot".to_string(),
                                ));
                            }

                            // Read all components
                            let components = &class.unwrap().components;
                            let left_snapshot = T::none();
                            let entity_snapshot = left_snapshot.delta_read(components, reader)?;

                            cur_snapshot.0.insert(id, (meta, entity_snapshot));
                        }
                        join::Item::Both(id, &(ref left_meta, ref left_snapshot), _) => {
                            num_entities_read += 1;

                            // This entity exists in both snapshots
                            assert!(id != INVALID_ENTITY_ID);

                            // Update existing entity snapshot with delta from the stream
                            let components = &classes.0[&left_meta.class_id].components;
                            let entity_snapshot = left_snapshot.delta_read(components, reader)?;

                            cur_snapshot
                                .0
                                .insert(id, (left_meta.clone(), entity_snapshot));
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

        stats::record("tick num entities", num_entities_read as f32);

        Ok((new_entities, cur_snapshot))
    }

    /// Calculate some kind of measure of how different two world snapshots are, restricted to the
    /// entities that exist in both snapshots.
    pub fn distance(&self, other: &Self) -> Result<f32, repl::Error> {
        let mut dist = 0.0f32;

        for join_item in join::FullJoinIter::new(self.0.iter(), other.0.iter()) {
            match join_item {
                join::Item::Both(
                    &_id,
                    &(ref left_meta, ref left_snapshot),
                    &(ref right_meta, ref right_snapshot),
                ) => {
                    // This entity exists in the left and the right snapshot
                    assert!(left_meta == right_meta);

                    dist += left_snapshot.distance(right_snapshot)?;
                }
                _ => {}
            }
        }

        Ok(dist)
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
            use bit_manager::{self, BitRead, BitWrite};

            use specs::prelude::{Entities, System, ReadStorage, WriteStorage, Fetch, Join};

            use defs::PlayerId;
            use entity::Meta;
            use repl::{self, snapshot};

            $(use $use_head$(::$use_tail)*;)*

            /// All components that can be replicated with an EntitySnapshot.
            #[derive(Clone, Copy, PartialEq, Debug)]
            pub enum ComponentType {
                $(
                    $field_type,
                )+
            }

            impl snapshot::ComponentType for ComponentType {
                type EntitySnapshot = EntitySnapshot;
            }

            /// Build an entity with a given list of component types.
            /*pub fn build(components: &[ComponentType], mut builder: EntityBuilder) {
                for component in component {
                    match component {
                        $(
                            &ComponentType::$field_type => {
                                builder = builder.with::<$field_type>(Default::default());
                            }
                        )+
                    }
                }
                builder
            }*/

            /// Complete replicated state of one entity. Note that not every component needs to be
            /// given for every entity.
            #[derive(Clone, PartialEq)]
            pub struct EntitySnapshot {
                $(
                    pub $field_name: Option<$field_type>,
                )+
            }

            $(
                impl snapshot::HasComponent<$field_type> for EntitySnapshot {
                    fn get(&self) -> Option<$field_type> {
                        self.$field_name.clone()
                    }
                }
            )+

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
                ) -> Result<(), bit_manager::Error> {
                    for component in components {
                        match *component {
                            $(
                                ComponentType::$field_type => {
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
                ) -> Result<Self, snapshot::Error> {
                    let mut result = Self::none();

                    for component in components {
                        match *component {
                            $(
                                ComponentType::$field_type => {
                                    let changed = reader.read_bit()?;

                                    if changed {
                                        // Component has changed, so read the updated value
                                        result.$field_name = Some(reader.read()?);
                                    } else {
                                        // Component has not changed, so take the previous value
                                        if self.$field_name.is_none() {
                                            return Err(snapshot::Error::ReceivedInvalidSnapshot(
                                                format!(
                                                    "previous snapshot is missing component {} \
                                                     for entity with repl components {:?}",
                                                    stringify!($field_name),
                                                    components,
                                                )
                                            ));
                                        }
                                        result.$field_name = self.$field_name.clone();
                                    }
                                }
                            )+
                        }
                    }

                    Ok(result)
                }

                fn distance(&self, other: &EntitySnapshot) -> Result<f32, repl::Error> {
                    let mut max_dist = 0.0f32;

                    $(
                        match (&self.$field_name, &other.$field_name) {
                            (&Some(ref a), &Some(ref b)) => {
                                let dist = repl::Predictable::distance(a, b);
                                if dist > 0.0 {
                                    //debug!("{}: {}", stringify!($field_name), dist);
                                }
                                max_dist = max_dist.max(dist);
                            }
                            (&None, &None) => {},
                            _ => {
                                return Err(repl::Error::Replication(
                                    format!(
                                        "component {} is given only for one of the entities\
                                         while calculating entity distance",
                                        stringify!($field_name)
                                    )
                                ));
                            }
                        }
                    )+

                    Ok(max_dist)
                }
            }

            pub type EntityClass = snapshot::EntityClass<EntitySnapshot>;
            pub type EntityClasses = snapshot::EntityClasses<EntitySnapshot>;
            pub type WorldSnapshot = snapshot::WorldSnapshot<EntitySnapshot>;

            /// System data for loading an entity snapshot
            pub type LoadData<'a> = (
                $(
                    ReadStorage<'a, $field_type>,
                )+
            );

            /// System data for storing an entity snapshot
            pub type StoreData<'a> = (
                $(
                    WriteStorage<'a, $field_type>,
                )+
            );

            /// Store World state of entities with ReplId component in a Snapshot.
            pub struct StoreSnapshotSys {
                pub snapshot: WorldSnapshot,
                pub only_player: Option<PlayerId>,
            }

            impl<'a> System<'a> for StoreSnapshotSys {
                type SystemData = (
                    Fetch<'a, EntityClasses>,
                    Entities<'a>,
                    ReadStorage<'a, repl::Id>,
                    ReadStorage<'a, Meta>,
                    LoadData<'a>,
                );

                fn run(
                    &mut self,
                    (classes, entities, repl_id, meta, ($($field_name,)+)): Self::SystemData,
                ) {
                    self.snapshot.0.clear();

                    let join = (&*entities, &repl_id, &meta).join();
                    for (entity, repl_id, meta) in join {
                        if let Some(only_player) = self.only_player {
                            if (repl_id.0).0 != only_player {
                                continue;
                            }
                        }

                        let components = &classes.0.get(&meta.class_id).unwrap().components;

                        let mut entity_snapshot: EntitySnapshot = snapshot::EntitySnapshot::none();
                        for component in components {
                            match *component {
                                $(
                                    ComponentType::$field_type => entity_snapshot.$field_name =
                                        Some($field_name.get(entity).unwrap().clone()),
                                )+
                            }
                        }

                        self.snapshot.0.insert(repl_id.0, (meta.clone(), entity_snapshot));
                    }
                }
            }

            /// Overwrite World state of entities with `ReplId` component with the state in a
            /// Snapshot. Note that this system does not create new entities.
            pub struct LoadSnapshotSys<'a> {
                pub snapshot: &'a WorldSnapshot,
                pub exclude_player: Option<PlayerId>,
                pub only_player: Option<PlayerId>,
            }

            impl<'a> System<'a> for LoadSnapshotSys<'a> {
                type SystemData = (
                    Fetch<'a, repl::EntityMap>,
                    StoreData<'a>,
                );

                fn run(&mut self, (entity_map, ($(mut $field_name,)+)): Self::SystemData) {
                    for (&entity_id, entity_snapshot) in &self.snapshot.0 {
                        if let Some(exclude_player) = self.exclude_player {
                            if entity_id.0 == exclude_player {
                                continue;
                            }
                        }
                        if let Some(only_player) = self.only_player {
                            if entity_id.0 != only_player {
                                continue;
                            }
                        }

                        let entity = entity_map.id_to_entity(entity_id);

                        $(
                            if let Some(component) = (entity_snapshot.1).$field_name.as_ref() {
                                // At this point, the snapshot is no longer a delta and contains
                                // state of all repl entities that this client knows. Since we
                                // don't want to unnecessary flag the component in specs as
                                // changed, we have to check here if its value has changed. It is
                                // unlikely that this will ever be a bottleneck unless we have
                                // boatloads of entities.
                                let changed =
                                    if let Some(prev_component) = $field_name.get(entity) {
                                        component != prev_component
                                    } else {
                                        true
                                    };

                                if changed {
                                    $field_name.insert(entity, component.clone());
                                }
                            }
                        )+
                    }
                }
            }
        }
    }
}

/// This macro generates a `Snapshot` struct to be able to copy the state of a
/// selection of components from a `specs::World`. We only look at entities
/// that have a `ReplId` component, which stores the unique EntityId shared by
/// the server and all clients. 
///
/// The macro requires a list of names and types of the components to be stored.
/// The components are assumed to implement Component, Clone, Serialize and
/// Deserialize.
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
            use std::collections::HashMap;

            use specs::{Component, Entity, Entities, VecStorage, HashMapStorage, System, ReadStorage, WriteStorage, Fetch, Join, World};
            use defs::EntityId;
            use repl::{ReplId, ReplEntities};

            $(use $use_head$(::$use_tail)*;)*

            // Complete replicated state of one entity. Note that not every
            // component needs to be given for every entity.
            pub struct EntitySnapshot {
                $(
                    pub $field_name: Option<$field_type>,
                )+
            }

            impl EntitySnapshot {
                pub fn new() -> Self {
                    Self {
                        $(
                            $field_name: None,
                        )+
                    }
                }
            }

            // Stores state of a selection of components
            pub struct Snapshot {
                // TODO: More efficient representation.
                pub entities: HashMap<EntityId, EntitySnapshot>,
            }

            // Store World state of entities with ReplId component in a Snapshot
            pub struct StoreSnapshotSys<'a>(pub &'a mut Snapshot);

            impl<'a> System<'a> for StoreSnapshotSys<'a> {
                type SystemData = (Entities<'a>,
                                   ReadStorage<'a, ReplId>,
                                   $(
                                       ReadStorage<'a, $field_type>,
                                   )+);

                fn run(&mut self, (entities, repl_id, $($field_name,)+): Self::SystemData) {
                    self.0.entities.clear();

                    for (entity, repl_id) in (&*entities, &repl_id).join() {
                        let entity_snapshot = EntitySnapshot {
                            $(
                                $field_name: $field_name.get(entity).map(|c| c.clone()),
                            )+
                        };
                        self.0.entities.insert(repl_id.0, entity_snapshot);
                    }
                }
            }

            // Overwrite World state of entities with ReplId component with the state in a Snapshot
            pub struct LoadSnapshotSys<'a>(pub &'a Snapshot);

            impl<'a> System<'a> for LoadSnapshotSys<'a> {
                type SystemData = (Fetch<'a, ReplEntities>,
                                   $(
                                       WriteStorage<'a, $field_type>,
                                   )+);

                fn run(&mut self, (repl_entities, $(mut $field_name,)+): Self::SystemData) {
                    for (&entity_id, entity_snapshot) in self.0.entities.iter() {
                        let entity = repl_entities.id_to_entity(entity_id);

                        $(
                            if let &Some(ref component) = &entity_snapshot.$field_name {
                                $field_name.insert(entity, component.clone());
                            }
                        )+
                    }
                }
            }

            // Serialize Snapshots
            impl Snapshot {

            }
        }
    }
}

snapshot! {
    use physics::Position;
    use physics::Orientation;

    mod net_repl {
        position: Position,
        orientation: Orientation,
    }
}

use specs::{World, RunNow};
use std::borrow::Borrow;
pub fn f(x: &mut net_repl::Snapshot, y: &mut World) {
    net_repl::StoreSnapshotSys(x).run_now(&mut y.res);
}

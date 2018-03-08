use std::marker::PhantomData;

use specs::{Component, Entities, Fetch, Join, ReadStorage, System, VecStorage, WriteStorage};

use defs::PlayerId;
use entity::Active;
use repl;
use repl::snapshot::{EntitySnapshot, HasComponent, WorldSnapshot};

use hooks_util::join;

pub trait Interp {
    fn interp(&self, other: &Self, t: f32) -> Self;
}

#[derive(Clone, Debug)]
pub struct State<C>(C, C);

impl<C: Component + Send + Sync> Component for State<C> {
    type Storage = VecStorage<Self>;
}

/// Load the state of component `C` from two snapshots, making it possible to interpolate between
/// the state at two times.
///
/// Note that this system assumes that all the entities present in the left snapshot have already
/// been created. The client handles this by calling `repl::entity::create_new_entities` on the
/// left tick when starting it.
pub struct LoadStateSys<'a, T: EntitySnapshot, C> {
    left: &'a WorldSnapshot<T>,
    right: &'a WorldSnapshot<T>,
    exclude_player: Option<PlayerId>,
    phantom: PhantomData<C>,
}

impl<'a, T: EntitySnapshot, C> LoadStateSys<'a, T, C> {
    pub fn new(
        left: &'a WorldSnapshot<T>,
        right: &'a WorldSnapshot<T>,
        exclude_player: Option<PlayerId>,
    ) -> Self {
        LoadStateSys {
            left,
            right,
            exclude_player,
            phantom: PhantomData,
        }
    }
}

impl<'a, T, C> System<'a> for LoadStateSys<'a, T, C>
where
    T: EntitySnapshot + HasComponent<C>,
    C: Component + Send + Sync + Clone,
{
    type SystemData = (
        Fetch<'a, repl::EntityMap>,
        ReadStorage<'a, Active>,
        WriteStorage<'a, State<C>>,
    );

    fn run(&mut self, (entity_map, active, mut states): Self::SystemData) {
        // Make sure to forget about entities that no longer exist.
        // TODO: This could definitely be done more efficiently.
        //       Maybe use Option in State?
        states.clear();

        for item in join::FullJoinIter::new(self.left.0.iter(), self.right.0.iter()) {
            let (id, left_state, right_state) = match item {
                join::Item::Both(&id, &(_, ref left_state), &(_, ref right_state)) => {
                    // Load interpolation state
                    (id, left_state, right_state)
                }
                join::Item::Left(&id, &(_, ref left_state)) => {
                    // Entity will be removed in the next tick, so let's just fix it at the current
                    // position.
                    (id, left_state, left_state)
                }
                join::Item::Right(_, _) => {
                    // Entity does not exist in the left snapshot yet, i.e. it will only be created
                    // in the next tick. Ignore.
                    continue;
                }
            };

            if let Some(exclude_player) = self.exclude_player {
                if id.0 == exclude_player {
                    continue;
                }
            }

            if let Some(left_state) = HasComponent::<C>::get(left_state) {
                // Due to events from intermediate ticks following the last snapshot tick, the
                // entity might already have been removed.
                if let Some(entity) = entity_map.get_id_to_entity(id) {
                    if active.get(entity).is_none() {
                        // Entity is currently disabled, so ignore in interpolation
                        continue;
                    }

                    // The repl components of an entity do not change in its lifetime. Hence, it
                    // would be a bug in delta deserialization if the right entity does not have
                    // this component anymore.
                    let right_state = right_state.get().unwrap();

                    states.insert(entity, State(left_state, right_state));
                }
            }
        }
    }
}

/// Interpolate between the states loaded for component `C`.
pub struct InterpSys<C>(f32, PhantomData<C>);

impl<C> InterpSys<C> {
    pub fn new(t: f32) -> Self {
        InterpSys(t, PhantomData)
    }
}

impl<'a, C> System<'a> for InterpSys<C>
where
    C: Component + Send + Sync + Interp,
{
    type SystemData = (
        Entities<'a>,
        ReadStorage<'a, Active>,
        ReadStorage<'a, State<C>>,
        WriteStorage<'a, C>,
    );

    fn run(&mut self, (entities, active, state, mut output): Self::SystemData) {
        for (entity, _active, state) in (&*entities, &active, &state).join() {
            output.insert(entity, state.0.interp(&state.1, self.0));
        }
    }
}

use std::collections::BTreeMap;
use std::f32;

use nalgebra::norm_squared;
use specs::prelude::*;

use hooks_util::{join, stats};

use defs::{PlayerId, PlayerInput, TickNum};
use event;
use game::{self, input};
use physics::{self, Orientation, Position};
use repl::interp::Interp;
use repl::{self, tick, EntityMap};

const MIN_SNAP_DISTANCE: f32 = 80.0;
const MIN_SNAP_ANGLE: f32 = f32::consts::PI / 4.0;
const TAU: f32 = 0.1;

struct LogEntry {
    input: PlayerInput,

    /// Snapshot of our entities after executing the input.
    /// We will use this to calculate an error for the prediction when the correct snapshot arrives
    /// from the server (not implemented yet).
    snapshot: game::WorldSnapshot,
}

pub struct Log {
    my_player_id: PlayerId,
    entries: BTreeMap<TickNum, LogEntry>,
}

impl Log {
    pub fn new(my_player_id: PlayerId) -> Log {
        Log {
            my_player_id,
            entries: BTreeMap::new(),
        }
    }

    /// Reset player entity state as present in the server's snapshot.
    fn reset(&self, world: &World, auth_snapshot: &game::WorldSnapshot) {
        // First snap everything to the auth snapshot
        let mut sys = game::LoadSnapshotSys {
            snapshot: auth_snapshot,
            exclude_player: None,
            only_player: Some(self.my_player_id),
        };
        sys.run_now(&world.res);
    }

    fn smooth(
        &self,
        world: &World,
        our_snapshot: &game::WorldSnapshot,
        auth_snapshot: &game::WorldSnapshot,
    ) {
        let entity_map = world.write_resource::<EntityMap>();
        let mut positions = world.write_storage::<Position>();
        let mut orientations = world.write_storage::<Orientation>();

        for item in join::FullJoinIter::new(our_snapshot.0.iter(), auth_snapshot.0.iter()) {
            match item {
                join::Item::Both(&id, &(_, ref left_state), &(_, ref right_state)) => {
                    // Since we are applying smoothing in the past, the entity might not actually
                    // be alive anymore. Consider e.g. the case of a player dying but still
                    // receiving prediction corrections for inputs of a previous tick.
                    // (On second thought, I think this would only happen if we somehow predicted
                    // entity removal.)
                    if let Some(entity) = entity_map.get_id_to_entity(id) {
                        match (left_state.velocity, right_state.velocity) {
                            (Some(p_left), Some(p_right)) => {
                                let dist = norm_squared(&(p_right.0 - p_left.0));
                                //println!("dist {}", dist.sqrt());
                                stats::record("vel dist", dist.sqrt());
                                //if dist > 0.000001 {
                                println!("vel dist {}", dist.sqrt());
                                //}
                            }
                            _ => {}
                        }
                        match (left_state.position, right_state.position) {
                            (Some(p_left), Some(p_right)) => {
                                let dist = norm_squared(&(p_right.0 - p_left.0));
                                if dist > 0.000001 {
                                    //println!("dist {}", dist.sqrt());
                                    stats::record("dist", dist.sqrt());
                                }
                                let p = if dist >= MIN_SNAP_DISTANCE * MIN_SNAP_DISTANCE {
                                    p_right
                                } else {
                                    p_left.interp(&p_right, TAU)
                                };
                                positions.insert(entity, p);
                            }
                            _ => {}
                        }
                        match (left_state.orientation, right_state.orientation) {
                            (Some(orientation_a), Some(orientation_b)) => {
                                // TODO: Angle normalization
                                let dist = (orientation_b.0 - orientation_a.0).abs();
                                let o = if dist > MIN_SNAP_ANGLE {
                                    orientation_b
                                } else {
                                    orientation_a.interp(&orientation_b, TAU)
                                };
                                orientations.insert(entity, o);
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn record(&mut self, world: &World, tick: TickNum, input: &PlayerInput) {
        // Snapshot the predicted state of our entities
        let mut sys = game::StoreSnapshotSys {
            snapshot: game::WorldSnapshot::new(),
            only_player: Some(self.my_player_id),
        };
        sys.run_now(&world.res);

        self.entries.insert(
            tick,
            LogEntry {
                input: input.clone(),
                snapshot: sys.snapshot,
            },
        );
    }

    fn correct(
        &mut self,
        world: &mut World,
        physics_runner: &mut physics::sim::Runner,
        tick_data: &tick::Data<game::EntitySnapshot>,
    ) -> Result<(), repl::Error> {
        if let Some(last_input_tick) = tick_data.last_input_tick {
            //debug!("got correction for {}", last_input_tick);

            // The server informs us that this `tick_data` contains state after executing our input
            // number `last_input_tick`.

            // Forget older log entries
            for &log_input_tick in &self.entries.keys().cloned().collect::<Vec<_>>() {
                if log_input_tick < last_input_tick {
                    self.entries.remove(&log_input_tick);
                }
            }

            // If the tick data contains a snapshot, we can correct our prediction
            if let Some(auth_snapshot) = tick_data.snapshot.as_ref() {
                // Calculate prediction error
                let our_snapshot = &self.entries
                    .get(&last_input_tick)
                    .ok_or_else(|| {
                        repl::Error::Replication(format!(
                            "Received prediction correction for input num {}\
                             but we have no log entry for that",
                            last_input_tick,
                        ))
                    })?
                    .snapshot;

                let distance = our_snapshot.distance(&auth_snapshot)?;

                stats::record("prediction error", distance);

                let replay = true;

                if replay {
                    // Reset to auth state of player entities
                    //debug!("resetting");
                    self.reset(world, auth_snapshot);
                    //self.smooth(world, our_snapshot, auth_snapshot);

                    // Now apply our recorded inputs again
                    for (&log_input_tick, log_entry) in &self.entries {
                        // TODO
                        if log_input_tick <= last_input_tick {
                            continue;
                        }

                        //debug!("replaying {}", log_input_tick);

                        input::auth::run_player_input(
                            world,
                            physics_runner,
                            &[(self.my_player_id, log_entry.input.clone())],
                        )?;
                    }
                }
            }
        } else {
            // The server has not executed any of our inputs yet.
            // NOTE: It is important that we load the initial snapshot for player entities.
            //       The reason is that, when prediction is enabled, `game::run::ViewRunner`
            //       ignores player-owned entities when loading the server's snapshots.
            //       Not sure if this is the best place to load the initial state.
            if let Some(auth_snapshot) = tick_data.snapshot.as_ref() {
                self.reset(world, auth_snapshot);
            }
        }

        Ok(())
    }

    pub fn run(
        &mut self,
        world: &mut World,
        physics_runner: &mut physics::sim::Runner,
        tick: TickNum,
        tick_data: &tick::Data<game::EntitySnapshot>,
        input: &PlayerInput,
    ) -> Result<(), repl::Error> {
        self.correct(world, physics_runner, tick_data)?;

        // For now, just ignore any events emitted locally in prediction.
        // TODO: This will need to be refined. Might want to predict only some events.
        let ignore = world.write_resource::<event::Sink>().set_ignore(true);

        input::auth::run_player_input(
            world,
            physics_runner,
            &[(self.my_player_id, input.clone())],
        )?;
        //debug!("running {}", tick);

        world.write_resource::<event::Sink>().set_ignore(ignore);

        self.record(world, tick, input);

        Ok(())
    }
}

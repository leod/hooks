use std::collections::BTreeMap;
use std::f32;

use nalgebra::norm_squared;
use specs::prelude::*;

use hooks_util::{join, stats};

use defs::{PlayerId, PlayerInput, TickNum};
use event;
use physics::{self, Orientation, Position};
use repl::snapshot::{EntitySnapshot, WorldSnapshot}
use repl::{self, tick, EntityMap};
use run::common;

const MIN_SNAP_DISTANCE: f32 = 80.0;
const MIN_SNAP_ANGLE: f32 = f32::consts::PI / 4.0;
const TAU: f32 = 0.1;

/// An entry in the prediction history.
struct Entry<T: EntitySnapshot> {
    input: PlayerInput,

    /// Snapshot of our entities after executing the input.
    /// We will use this to calculate an error for the prediction when the correct snapshot arrives
    /// from the server (not implemented yet).
    snapshot: WorldSnapshot<T>,
}

pub struct History {
    my_player_id: PlayerId,
    entries: BTreeMap<TickNum, Entry>,
}

impl History {
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
        run_common: &mut common::Run,
        tick_data: &tick::Data<game::EntitySnapshot>,
    ) -> Result<(), repl::Error> {
        if let Some(last_input_tick) = tick_data.last_input_tick {
            // The server informs us that this `tick_data` contains state after executing our input
            // number `last_input_tick`.

            //debug!("got correction for {}", last_input_tick);

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
                    self.reset(run_common.world(), auth_snapshot);
                    //self.smooth(world, our_snapshot, auth_snapshot);

                    // Now apply our recorded inputs again
                    for (&log_input_tick, log_entry) in &self.entries {
                        // TODO
                        if log_input_tick <= last_input_tick {
                            continue;
                        }

                        //debug!("replaying {}", log_input_tick);

                        let input = [(self.my_player_id, log_entry.input.clone())];
                        run_common.run_player_input(&input)?;
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
                self.reset(run_common.world(), auth_snapshot);
            }
        }

        Ok(())
    }

    pub fn run(
        &mut self,
        run_common: common::Run,
        tick: TickNum,
        tick_data: &tick::Data<game::EntitySnapshot>,
        input: &PlayerInput,
    ) -> Result<(), repl::Error> {
        self.correct(run_common, tick_data)?;

        run_common.run_player_input(&[(self.my_player_id, input.clone())])?;

        self.record(world, tick, input);

        Ok(())
    }
}

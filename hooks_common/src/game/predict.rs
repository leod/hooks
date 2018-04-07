use std::collections::BTreeMap;

use specs::prelude::{RunNow, World};

use defs::{PlayerId, PlayerInput, TickNum};
use event;
use game::{self, input};
use physics;
use repl::{self, tick};

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
        let mut sys = game::LoadSnapshotSys {
            snapshot: auth_snapshot,
            exclude_player: None,
            only_player: Some(self.my_player_id),
        };
        sys.run_now(&world.res);
    }

    fn record(&mut self, world: &World, tick_num: TickNum, input: &PlayerInput) {
        // Snapshot the predicted state of our entities
        let mut sys = game::StoreSnapshotSys {
            snapshot: game::WorldSnapshot::new(),
            only_player: Some(self.my_player_id),
        };
        sys.run_now(&world.res);

        self.entries.insert(
            tick_num,
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
        if let Some(last_input_num) = tick_data.last_input_num {
            //debug!("got correction for {}", last_input_num);

            // The server informs us that this `tick_data` contains state after executing our input
            // number `last_input_num`.

            // Forget older log entries
            for &log_input_num in &self.entries.keys().cloned().collect::<Vec<_>>() {
                if log_input_num < last_input_num {
                    self.entries.remove(&log_input_num);
                }
            }

            // If the tick data contains a snapshot, we can correct our prediction
            if let Some(auth_snapshot) = tick_data.snapshot.as_ref() {
                // Calculate prediction error
                let distance = if let Some(log_entry) = self.entries.get(&last_input_num) {
                    log_entry.snapshot.distance(&auth_snapshot)?
                } else {
                    return Err(repl::Error::Replication(format!(
                        "Received prediction correction for input num {}\
                         but we have no log entry for that",
                        last_input_num,
                    )));
                };

                if distance > 0.0 {
                    debug!("prediction error: {}", distance);
                }

                let replay = true;

                if replay {
                    // Reset to auth state of player entities
                    //debug!("resetting");
                    self.reset(world, auth_snapshot);

                    // Now apply our recorded inputs again
                    for (&log_input_num, log_entry) in &self.entries {
                        // TODO
                        if log_input_num <= last_input_num {
                            continue;
                        }

                        //debug!("replaying {}", log_input_num);

                        input::auth::run_player_input(
                            world,
                            physics_runner,
                            self.my_player_id,
                            &log_entry.input,
                        )?;
                    }
                }
            }
        } else {
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
        tick_num: TickNum,
        tick_data: &tick::Data<game::EntitySnapshot>,
        input: &PlayerInput,
    ) -> Result<(), repl::Error> {
        self.correct(world, physics_runner, tick_data)?;

        // For now, just ignore any events emitted locally in prediction.
        // TODO: This will need to be refined. Might want to predict only some events.
        let ignore = world.write_resource::<event::Sink>().set_ignore(true);

        input::auth::run_player_input(world, physics_runner, self.my_player_id, input)?;
        //debug!("running {}", tick_num);

        world.write_resource::<event::Sink>().set_ignore(ignore);

        self.record(world, tick_num, input);

        Ok(())
    }
}

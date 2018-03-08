use std::collections::BTreeMap;

use specs::{RunNow, World};

use defs::{PlayerId, PlayerInput, TickNum};
use repl::{self, tick};
use game::{self, input};

struct LogEntry {
    input: PlayerInput,

    /// Snapshot of our entities after executing the input.
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
        tick_data: &tick::Data<game::EntitySnapshot>,
    ) -> Result<(), repl::Error> {
        if let Some(last_input_num) = tick_data.last_input_num {
            debug!("got correction for {}", last_input_num);

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
                let replay = true;

                if replay {
                    // Reset to auth state of player entities
                    debug!("resetting");
                    self.reset(world, auth_snapshot);

                    // Now apply our recorded inputs again
                    for (&log_input_num, log_entry) in &self.entries {
                        // TODO
                        if log_input_num <= last_input_num {
                            continue;
                        }

                        debug!("replaying {}", log_input_num);

                        input::auth::run_player_input(world, self.my_player_id, &log_entry.input)?;
                    }
                }
            }
        } else {
            // TODO: It is important that we load the initial snapshot for player entities.
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
        tick_num: TickNum,
        tick_data: &tick::Data<game::EntitySnapshot>,
        input: &PlayerInput,
    ) -> Result<(), repl::Error> {
        self.correct(world, tick_data)?;

        input::auth::run_player_input(world, self.my_player_id, input)?;
        debug!("running {}", tick_num);

        self.record(world, tick_num, input);

        Ok(())
    }
}

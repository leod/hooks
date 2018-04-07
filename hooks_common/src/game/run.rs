use specs::prelude::{RunNow, World};

use hooks_util::profile;

use defs::{PlayerId, PlayerInput, TickNum};
use entity;
use event::{self, Event};
use game::state::State;
use game::{self, input, predict};
use physics;
use repl::{self, tick};

struct CommonRunner {
    physics_runner: physics::sim::Runner,
}

impl CommonRunner {
    fn new(world: &mut World) -> CommonRunner {
        CommonRunner {
            physics_runner: physics::sim::Runner::new(world),
        }
    }

    /// Execute the deferred removal of entities tagged with `Remove`. Right now, we try to call
    /// this function after every step of the tick, with the hope of avoiding any interaction with
    /// removed entities in subsequent steps.
    fn perform_removals(&mut self, state: &mut State) {
        // Here, systems have a chance to react to entities that will be removed, tagged with the
        // `Remove` component ...
        state.removal_dispatcher.dispatch_seq(&state.world.res);

        // ... and now we go through with it.
        entity::perform_removals(&mut state.world);
    }

    fn run_pre_tick(&mut self, state: &mut State) -> Result<(), repl::Error> {
        // First run pre-tick event handlers, e.g. handle player join/leave events
        let events = state
            .world
            .read_resource::<event::Sink>()
            .clone()
            .into_vec();
        for event in &events {
            for handler in &state.pre_tick_event_handlers {
                handler(&mut state.world, &**event)?;
            }
        }

        self.perform_removals(state);

        for f in &state.pre_tick_fns {
            f(&mut state.world)?;
        }

        self.perform_removals(state);

        Ok(())
    }

    fn run_post_tick(&mut self, state: &mut State) -> Result<Vec<Box<Event>>, repl::Error> {
        let events = state.world.write_resource::<event::Sink>().clear();
        for event in &events {
            for handler in &state.post_tick_event_handlers {
                handler(&mut state.world, &**event)?;
            }
        }

        self.perform_removals(state);

        Ok(events)
    }

    fn run_tick(&mut self, state: &mut State) -> Result<(), repl::Error> {
        state.tick_dispatcher.dispatch_seq(&state.world.res);

        self.perform_removals(state);

        Ok(())
    }
}

pub struct AuthRunner {
    common: CommonRunner,
}

impl AuthRunner {
    pub fn new(world: &mut World) -> AuthRunner {
        AuthRunner {
            common: CommonRunner::new(world),
        }
    }

    /// Running a tick on the server side.
    pub fn run_tick(
        &mut self,
        state: &mut State,
        inputs: Vec<(PlayerId, PlayerInput)>,
    ) -> Result<Vec<Box<Event>>, repl::Error> {
        self.common.run_pre_tick(state)?;

        // TODO: For now, just run everyone's input here. This might need to get refined!
        for (player_id, input) in inputs {
            // Replication error on the server side is a bug, so unwrap
            input::auth::run_player_input(
                &mut state.world,
                &mut self.common.physics_runner,
                player_id,
                &input,
            ).unwrap();
        }

        self.common.run_tick(state)?;
        self.common.run_post_tick(state)
    }
}

pub struct ViewRunner {
    common: CommonRunner,
    my_player_id: PlayerId,
    predict_log: Option<predict::Log>,
}

impl ViewRunner {
    pub fn new(world: &mut World, my_player_id: PlayerId, predict: bool) -> ViewRunner {
        ViewRunner {
            common: CommonRunner::new(world),
            my_player_id,
            predict_log: if predict {
                Some(predict::Log::new(my_player_id))
            } else {
                None
            },
        }
    }

    /// Running a tick on the client side. We try to do things in the same order on the clients as
    /// on the server, which is why we have put these functions in the same module here.
    pub fn run_tick(
        &mut self,
        state: &mut State,
        tick_num: TickNum,
        tick_data: &tick::Data<game::EntitySnapshot>,
        input: &PlayerInput,
    ) -> Result<Vec<Box<Event>>, repl::Error> {
        profile!("run");

        let events = event::Sink::clone_from_vec(&tick_data.events);
        state.push_events(events.into_vec());

        self.common.run_pre_tick(state)?;

        if let Some(ref snapshot) = tick_data.snapshot {
            profile!("load");

            // By now we are up-to-date regarding the player list, so we can create new entities
            repl::entity::view::create_new_entities(&mut state.world, snapshot)?;

            // Snap entities to their state in the new tick
            let mut sys = game::LoadSnapshotSys {
                snapshot,
                exclude_player: if self.predict_log.is_some() {
                    Some(self.my_player_id)
                } else {
                    None
                },
                only_player: None,
            };
            sys.run_now(&state.world.res);
        }

        // Prediction
        // TODO: Should this happen before or after loading snapshots?
        // TODO: Consider input frequency > tick frequency?
        if let Some(predict_log) = self.predict_log.as_mut() {
            profile!("predict");

            predict_log.run(
                &mut state.world,
                &mut self.common.physics_runner,
                tick_num,
                tick_data,
                input,
            )?;
        }

        self.common.run_tick(state)?;
        self.common.run_post_tick(state)
    }

    pub fn predict(&self) -> bool {
        self.predict_log.is_some()
    }
}

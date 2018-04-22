use run::{common, State};

type TickFn = fn(&mut World);

#[derive(Default)]
struct TickSetup {
    pre_tick_fns: Vec<TickFn>,
    tick_systems: Vec<Box<System<'static>>>,
}

#[derive(Default)]
pub struct Setup {
    tick_setup: TickSetup,
    pub common: run::common::Setup,
}

impl Setup {
    pub fn new() -> Setup {
        Default::default()
    }

    pub fn pre_tick_fn(&mut self, f: TickFn) {
        self.tick_setup.pre_tick_fns.push(f);
    }

    pub fn tick_system<T>(&mut self, system: T)
    where
        T: System<'static> + Send + 'static,
    {
        self.tick_setup.tick_systems.push(Box::new(system));
    }
}

struct Runner {
    tick_setup: TickSetup,
    common: common::Runner,
}

impl Runner {
    pub fn new(setup: Setup) -> Runner {
        Runner {
            tick_setup: setup.tick_setup,
            common: common::Runner::new(setup.common),
        }
    }

    fn run_pre_tick(&mut self, state: &mut State) {
        // Replication error on server side is a bug, so unwrap
        self.common.run_pre_tick(state).unwrap();

        for f in &state.pre_tick_fns {
            f(&mut state.world);
        }

        self.common.perform_removals(state);

        Ok(())
    }

    /// Running a tick on the server side.
    pub fn run_tick(
        &mut self,
        state: &mut State,
        external_events: Vec<Box<Event>>,
        input_batches: Vec<Vec<(PlayerId, PlayerInput)>>,
    ) -> Vec<Box<Event>> {
        self.run_pre_tick();

        for 

        for inputs in input_batches {
            input::auth::run_player_input(
                &mut state.world,
                &mut self.common.physics_runner,
                &inputs,
            ).unwrap();
        }

        self.common.run_post_tick().unwrap();
    }
}

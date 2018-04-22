use specs::prelude::*;

use defs::{PlayerId, PlayerInput};
use event::Event;
use run::common;
use registry::Registry;

type TickFn = fn(&mut World);

#[derive(Default)]
struct TickSetup {
    tick_fns: Vec<TickFn>,
}

#[derive(Default)]
pub struct Setup {
    tick_setup: TickSetup,
    pub common: common::Setup,
}

impl Setup {
    pub fn new() -> Setup {
        Default::default()
    }

    pub fn add_tick_fn(&mut self, f: TickFn) {
        self.tick_setup.tick_fns.push(f);
    }
}

pub struct Run {
    tick_setup: TickSetup,
    run_common: common::Run,
}

impl Run {
    pub fn new(registry: Registry, setup: Setup) -> Run {
        Run {
            tick_setup: setup.tick_setup,
            run_common: common::Run::new(registry, setup.common),
        }
    }

    pub fn world(&self) -> &World {
        self.run_common.world()
    }

    pub fn world_mut(&self) -> &mut World {
        self.run_common.world_mut()
    }

    pub fn event_registry(&self) -> &event::Registry {
        self.run_common.event_registry()
    }

    /// Run a tick on the server side.
    pub fn run_tick(
        &mut self,
        external_events: Vec<Box<Event>>,
        input_batches: Vec<Vec<(PlayerId, PlayerInput)>>,
    ) -> Vec<Box<Event>> {
        // We unwrap in this function since replication errors would be a bug on the server.

        // Run handlers for events that come from outside the game simulation.
        // Currently, this means handling player join / leave events.
        self.run_common.run_pre_tick(external_events).unwrap();

        for f in &state.tick_fns {
            f(self.world_mut());
        }

        for inputs in input_batches {
            self.run_common.run_player_input(&inputs).unwrap();
        }

        self.run_common.run_post_tick().unwrap()
    }
}

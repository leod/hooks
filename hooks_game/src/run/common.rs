//! Logic and state shared between server and clients for running a tick.

use specs::prelude::*;

use defs::{PlayerId, PlayerInput};
use event::Event;
use physics;
use repl;
use registry::Registry;

/// Game state.
pub struct State {
    pub world: World,
}

pub type InitFn = fn(&mut World);
pub type EventHandler = fn(&mut World, &Event) -> Result<(), repl::Error>;
pub type PlayerInputHandler = fn(
    &mut World,
    &mut physics::sim::Run, 
    &[(PlayerId, PlayerInput)],
) -> Result<(), repl::Error>;

/// Callbacks that are used when running a tick.
#[derive(Default)]
struct TickSetup {
    init_fns: Vec<InitFn>,
    event_handlers: Vec<EventHandler>,
    player_input_handlers: Vec<InputHandler>,
    removal_systems: Vec<Box<System<'static>>>,
}

/// The `Setup` is used to define which steps should run in a tick. Before starting the game,
/// modules can add callbacks into a `Setup` in their `register` function.
#[derive(Default>)]
pub struct Setup {
    tick_setup: TickSetup,
    pub physics: physics::sim::RunSetup,
}

impl Setup {
    pub fn new() -> Setup {
        Default::default()
    }

    pub fn add_init_fn(&mut self, f: InitFn) {
        self.init_fns.push(f);
    }

    pub fn add_event_handler(&mut self, f: EventHandler) {
        self.tick_setup.event_handlers.push(f);
    }

    pub fn add_player_input_handler(&mut self, f: PlayerInputHandler) {
        self.tick_setup.player_input_handlers.push(f);
    }

    pub fn add_removal_system<T>(&mut self, system: T)
    where
        T: System<'static> + Send + 'static,
    {
        self.tick_setup.removal_systems.push(Box::new(system));
    }

    fn init_state(&self, mut world: World) -> State {
        for f in self.init_fns {
            f(&mut world);
        }

        State {
            world,
        }
    }
}

pub(in run) struct Run {
    event_registry: event::Registry
    tick_setup: TickSetup,
    run_physics: physics::sim::Run,
    state: State,
}

impl Run {
    pub fn new(registry: Registry, setup: Setup) -> Run {
        let (world, event_registry) = registry.finalize();
        let state = setup.init_state(world);

        Run {
            event_registry,
            tick_setup: setup.tick_setup,
            run_physics: physics::sim::Run::new(world, setup.physics),
            state,
        }
    }

    /// Execute the deferred removal of entities tagged with `Remove`. Right now, we try to call
    /// this function after every step of the tick, with the hope of avoiding any interaction with
    /// removed entities in subsequent steps.
    fn perform_removals(&mut self) {
        // Here, systems have a chance to react to entities that will be removed, tagged with the
        // `Remove` component ...
        self.tick_setup.removal_dispatcher.dispatch_seq(&self.state.world.res);

        // ... and now we go through with it.
        entity::perform_removals(&mut self.state.world);
    }

    fn handle_events(
        &mut self,
        events: &[Box<Event>],
    ) -> Result<(), repl::Error> {
        for event in events {
            for handler in &self.event_handlers {
                handler(&mut self.state.world, &**event)?;
            }
        }

        Ok(())
    }

    pub fn world(&self) -> &World {
        &self.state.world
    }

    pub fn world_mut(&mut self) -> &mut World {
        &mut self.state.world
    }

    pub fn event_registry(&self) -> &event::Registry {
        &self.event_registry
    }

    /// Start the tick by handling events that came from outside of the local simulation.
    pub fn run_pre_tick(
        &mut self,
        external_events: &[Box<Event>],
    ) -> Result<(), repl::Error> {
        self.common.handle_events(external_events)?;

        // Remove entities that were marked with `Remove` during handling events
        self.common.perform_removals();

        Ok(())
    }

    pub fn run_player_input(
        &mut self,
        input: &[(PlayerId, PlayerInput)],
    ) -> Result<(), repl::Error> {
        for handler in &state.input_handlers {
            handler(&mut state.world, &mut self.run_physics, input)?;
        }
        Ok(())
    }

    /// End the tick by handling events that were generated during it.
    pub fn run_post_tick(&mut self) -> Result<Vec<Box<Event>>, repl::Error> {
        // Handle events that were generated locally in this tick
        let events = state.world.write_resource::<event::Sink>().clear();
        self.handle_events(&events);

        // At the end of the tick, remove entities that were marked as such.
        self.perform_removals();

        stats::record(
            "#players",
            state.world.read_resource::<repl::player::Players>().len() as f32,
        );

        Ok(events)
    }
}

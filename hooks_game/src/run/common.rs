use specs::prelude::*;

use defs::{PlayerId, PlayerInput};
use event::Event;
use physics;
use repl;
use run::State;

pub type EventHandler = fn(&mut World, &Event) -> Result<(), repl::Error>;
pub type PlayerInputHandler = fn(
    &mut World,
    &mut physics::sim::Runner, 
    &[(PlayerId, PlayerInput)],
) -> Result<(), repl::Error>;

#[derive(Default)]
struct TickSetup {
    event_handlers: Vec<EventHandler>,
    player_input_handlers: Vec<InputHandler>,
    removal_systems: Vec<Box<System<'static>>>,
}

#[derive(Default>)]
pub struct Setup {
    tick_setup: TickSetup,
    pub physics: physics::sim::RunnerSetup,
}

impl Setup {
    pub fn new() -> Setup {
        Default::default()
    }

    pub fn event_handler(&mut self, f: EventHandler) {
        self.tick_setup.event_handlers.push(f);
    }

    pub fn player_input_handler(&mut self, f: PlayerInputHandler) {
        self.tick_setup.player_input_handlers.push(f);
    }

    pub fn removal_system<T>(&mut self, system: T)
    where
        T: System<'static> + Send + 'static,
    {
        self.tick_setup.removal_systems.push(Box::new(system));
    }
}

pub(in run) struct Runner {
    tick_setup: TickSetup,
    physics: physics::sim::Runner,
}

impl Runner {
    pub fn new(world: &mut World, setup: Setup) -> CommonRunner {
        CommonRunner {
            tick_setup: setup.tick_setup,
            physics: physics::sim::Runner::new(world, setup.physics),
        }
    }

    /// Execute the deferred removal of entities tagged with `Remove`. Right now, we try to call
    /// this function after every step of the tick, with the hope of avoiding any interaction with
    /// removed entities in subsequent steps.
    pub fn perform_removals(&mut self, state: &mut State) {
        // Here, systems have a chance to react to entities that will be removed, tagged with the
        // `Remove` component ...
        tick_setup.removal_dispatcher.dispatch_seq(&state.world.res);

        // ... and now we go through with it.
        entity::perform_removals(&mut state.world);
    }

    pub fn run_pre_tick(&mut self, state: &mut State) -> Result<(), repl::Error> {
        // First run event handlers for events that came from outside the game, e.g. handle player
        // join/leave events
        let events = state
            .world
            .read_resource::<event::Sink>()
            .clone()
            .into_vec();
        for event in &events {
            for handler in &state.event_handlers {
                handler(&mut state.world, &**event)?;
            }
        }

        self.perform_removals(state);

        Ok(())
    }

    pub fn run_player_input(
        &mut self,
        state: &mut State,
        input: &[(PlayerId, PlayerInput)],
    ) -> Result<(), repl::Error> {
        for handler in &state.input_handlers {
            handler(&mut state.world, &mut self.physics, input)?;
        }

        self.perform_removals(state);

        Ok(())
    }

    pub fn run_post_tick(&mut self, state: &mut State) -> Result<Vec<Box<Event>>, repl::Error> {
        // TODO: run event handlers
        self.perform_removals(state);

        stats::record(
            "#players",
            state.world.read_resource::<repl::player::Players>().len() as f32,
        );

        Ok(events)
    }
}

use specs::{Dispatcher, RunNow, World};

use registry::{EventHandler, Registry, TickFn};
use event::{self, Event};
use entity;
use physics;
use repl::{self, tick};
use game;

pub struct State {
    pub world: World,
    pub event_reg: event::Registry,

    pub pre_tick_event_handlers: Vec<EventHandler>,
    pub pre_tick_fns: Vec<TickFn>,
    pub tick_dispatcher: Dispatcher<'static, 'static>,
    pub post_tick_event_handlers: Vec<EventHandler>,
    pub removal_dispatcher: Dispatcher<'static, 'static>,
}

impl State {
    pub fn from_registry(reg: Registry) -> State {
        State {
            world: reg.world,
            event_reg: reg.event_reg,

            pre_tick_event_handlers: reg.pre_tick_event_handlers,
            pre_tick_fns: reg.pre_tick_fns,
            tick_dispatcher: reg.tick_systems.build(),
            post_tick_event_handlers: reg.post_tick_event_handlers,
            removal_dispatcher: reg.removal_systems.build(),
        }
    }

    pub fn push_events(&self, events: Vec<Box<Event>>) {
        let mut sink = self.world.write_resource::<event::Sink>();

        for event in events.into_iter() {
            sink.push_box(event);
        }
    }

    fn perform_removals(&mut self) {
        // Here, systems have a chance to react to entities that will be removed, tagged with the
        // `Remove` component ...
        self.removal_dispatcher.dispatch_seq(&self.world.res);

        // ... and now we go through with it.
        entity::perform_removals(&mut self.world);
    }

    fn run_pre_tick(&mut self) -> Result<(), repl::Error> {
        // First run pre-tick event handlers, e.g. handle player join/leave events
        let events = self.world.read_resource::<event::Sink>().clone().into_vec();
        for event in &events {
            for handler in &self.pre_tick_event_handlers {
                handler(&mut self.world, &**event)?;
            }
        }

        for f in &self.pre_tick_fns {
            f(&mut self.world)?;
        }

        self.perform_removals();

        Ok(())
    }

    fn run_tick(&mut self) -> Result<(), repl::Error> {
        self.tick_dispatcher.dispatch_seq(&self.world.res);
        Ok(())
    }

    fn run_post_tick(&mut self) -> Result<Vec<Box<Event>>, repl::Error> {
        let events = self.world.write_resource::<event::Sink>().clear();
        for event in &events {
            for handler in &self.post_tick_event_handlers {
                handler(&mut self.world, &**event)?;
            }
        }

        self.perform_removals();

        Ok(events)
    }

    /// Running a tick on the server side.
    pub fn run_tick_auth(&mut self) -> Result<Vec<Box<Event>>, repl::Error> {
        self.run_pre_tick()?;
        self.run_tick()?;
        physics::sim::run(&self.world);
        self.run_post_tick()
    }

    /// Running a tick on the client side. We try to do things in the same order on the clients as
    /// on the server, which is why we have put these functions next to each other here.
    pub fn run_tick_view(
        &mut self,
        tick_data: &tick::Data<game::EntitySnapshot>,
    ) -> Result<Vec<Box<Event>>, repl::Error> {
        let events = event::Sink::clone_from_vec(&tick_data.events);
        self.push_events(events.into_vec());

        self.run_pre_tick()?;

        if let Some(ref snapshot) = tick_data.snapshot {
            // By now we are up-to-date regarding the player list, so we can create new entities
            repl::entity::view::create_new_entities(&mut self.world, snapshot)?;

            // Snap entities to their state in the new tick
            let mut sys = game::LoadSnapshotSys(snapshot);
            sys.run_now(&self.world.res);
        }

        self.run_tick()?;
        self.run_post_tick()
    }
}

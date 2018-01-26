use specs::{Dispatcher, RunNow, World};

use event::{self, Event};
use game;
use registry::{EventHandler, Registry, TickFn};
use repl::{self, entity, tick};

pub struct State {
    pub world: World,
    pub event_reg: event::Registry,

    pub event_handlers_pre_tick: Vec<EventHandler>,
    pub pre_tick_fns: Vec<TickFn>,
    pub tick_dispatcher: Dispatcher<'static, 'static>,
    pub event_handlers_post_tick: Vec<EventHandler>,
}

impl State {
    pub fn from_registry(reg: Registry) -> State {
        State {
            world: reg.world,
            event_reg: reg.event_reg,

            event_handlers_pre_tick: reg.event_handlers_pre_tick,
            pre_tick_fns: reg.pre_tick_fns,
            tick_dispatcher: reg.tick_systems.build(),
            event_handlers_post_tick: reg.event_handlers_post_tick,
        }
    }

    pub fn push_events(&self, events: Vec<Box<Event>>) {
        let mut sink = self.world.write_resource::<event::Sink>();

        for event in events.into_iter() {
            sink.push_box(event);
        }
    }

    /// Running a tick on the server side.
    pub fn run_tick_auth(&mut self) -> Result<Vec<Box<Event>>, repl::Error> {
        let events = self.world.read_resource::<event::Sink>().clone();
        for event in events.iter() {
            for handler in &self.event_handlers_pre_tick {
                handler(&mut self.world, &**event)?;
            }
        }

        for f in &self.pre_tick_fns {
            f(&mut self.world)?;
        }

        self.world.maintain();

        self.tick_dispatcher.dispatch_seq(&self.world.res);

        let events = self.world.write_resource::<event::Sink>().clear();
        for event in &events {
            for handler in &self.event_handlers_post_tick {
                handler(&mut self.world, &**event)?;
            }
        }

        Ok(events)
    }

    /// Running a tick on the client side. We try to do things in the same order on the clients as
    /// on the server, which is why we have put these functions next to each other here.
    pub fn run_tick_view(
        &mut self,
        tick_data: &tick::Data<game::EntitySnapshot>,
    ) -> Result<Vec<Box<Event>>, repl::Error> {
        let events = event::Sink::clone_from_vec(&tick_data.events);

        // TODO: Is it even necessary for clients to have the tick's events as a resource?
        self.push_events(events.into_vec());

        // First run pre-tick event handlers, e.g. handle player join/leave events
        for event in &tick_data.events {
            for handler in &self.event_handlers_pre_tick {
                handler(&mut self.world, &**event)?;
            }
        }

        for f in &self.pre_tick_fns {
            f(&mut self.world)?;
        }

        if let Some(ref snapshot) = tick_data.snapshot {
            // Now we are up-to-date regarding the player list, so we can create new entities
            entity::view::create_new_entities(&mut self.world, snapshot)?;

            self.world.maintain();

            // Snap entities to their state in the new tick
            let mut sys = game::LoadSnapshotSys(snapshot);
            sys.run_now(&self.world.res);
        } else {
            self.world.maintain();
        }

        // So far, there are no tick systems on the client. It's not clear yet if we will need
        // this.
        self.tick_dispatcher.dispatch_seq(&self.world.res);

        // Same for the post-tick event handlers. Do we need this?
        let events = self.world.write_resource::<event::Sink>().clear();
        for event in &events {
            for handler in &self.event_handlers_post_tick {
                handler(&mut self.world, &**event)?;
            }
        }

        Ok(events)
    }
}

use specs::{Dispatcher, World};

use event::{self, Event};
use registry::{EventHandler, Registry};
use repl;

pub struct State {
    pub world: World,
    pub event_reg: event::Registry,
    pub tick_dispatcher: Dispatcher<'static, 'static>,
    pub event_handlers_post_tick: Vec<EventHandler>,
}

impl State {
    pub fn from_registry(reg: Registry) -> State {
        State {
            world: reg.world,
            event_reg: reg.event_reg,
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

    pub fn run_tick(&mut self) -> Result<Vec<Box<Event>>, repl::Error> {
        self.tick_dispatcher.dispatch_seq(&self.world.res);

        let mut events = self.world.write_resource::<event::Sink>().clear();

        for event in &events {
            for handler in &self.event_handlers_post_tick {
                handler(&mut self.world, event)?;
            }
        }

        Ok(events)
    }
}

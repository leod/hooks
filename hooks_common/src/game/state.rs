use specs::{Dispatcher, World};

use registry::{EventHandler, Registry, TickFn};
use event::{self, Event};

pub struct State {
    pub world: World,
    pub event_reg: event::Registry,

    pub(in game) pre_tick_event_handlers: Vec<EventHandler>,
    pub(in game) pre_tick_fns: Vec<TickFn>,
    pub(in game) tick_dispatcher: Dispatcher<'static, 'static>,
    pub(in game) post_tick_event_handlers: Vec<EventHandler>,
    pub(in game) removal_dispatcher: Dispatcher<'static, 'static>,
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
}

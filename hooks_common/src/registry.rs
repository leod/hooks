use take_mut;

use bit_manager::data::BitStore;

use shred::Resource;
use specs::{Component, DispatcherBuilder, System, World};

use event::{self, Event};
use repl;

pub type EventHandler = fn(&mut World, &Box<Event>) -> Result<(), repl::Error>;

pub struct Registry {
    // These shouldn't be public, so please be so kind not to modify them:
    pub world: World,
    pub event_reg: event::Registry,
    pub tick_systems: DispatcherBuilder<'static, 'static>,
    pub event_handlers_post_tick: Vec<EventHandler>,
}

impl Registry {
    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn component<T: Component>(&mut self) {
        self.world.register::<T>();
    }

    pub fn resource<T: Resource>(&mut self, res: T) {
        self.world.add_resource(res);
    }

    pub fn resource_with_id<T: Resource>(&mut self, res: T, id: usize) {
        self.world.add_resource_with_id(res, id);
    }

    pub fn event<T: Event + BitStore + Send>(&mut self) {
        self.event_reg.register::<T>();
    }

    pub fn tick_system<T>(&mut self, system: T, name: &str, dep: &[&str])
    where
        T: for<'a> System<'a> + Send + 'static,
    {
        take_mut::take(&mut self.tick_systems, |tick_systems| {
            tick_systems.add(system, name, dep)
        });
    }

    pub fn event_handler_post_tick(&mut self, f: EventHandler) {
        self.event_handlers_post_tick.push(f);
    }
}

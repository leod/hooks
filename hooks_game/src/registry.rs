use bit_manager::data::BitStore;

use shred::Resource;
use specs::prelude::{Component, DispatcherBuilder, System, World};

use event::{self, Event};
use repl;

#[derive(Default)]
pub struct Registry {
    world: World,
    event_reg: event::Registry,
}

impl Registry {
    pub fn new() -> Registry {
        Default::default()
    }

    pub fn finalize(self) -> (World, event::Registry) {
        (self.world, self.event_reg)
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn component<T: Component>(&mut self)
    where
        T::Storage: Default,
    {
        self.world.register::<T>();
    }

    pub fn resource<T: Resource>(&mut self, res: T) {
        self.world.add_resource(res);
    }

    pub fn event<T: Event + BitStore + Send>(&mut self) {
        self.event_reg.register::<T>();
    }
}

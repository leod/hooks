use bit_manager::data::BitStore;

use shred::Resource;
use specs::prelude::{Component, DispatcherBuilder, System, World};

use event::{self, Event};
use repl;

/// We use the `Registry` to give modules a chance to register their own custom types before
/// starting a game. The main idea is to be able to decentralize registration without too much
/// effort.
///
/// For this purpose, our convention is that modules should define a `register` function, in which
/// their components, resources and event types are registered in the given `Registry`. In some
/// cases, resources or components may be necessary only on the server or client side. In this case,
/// the convention is to define nested modules `auth` and `view` with separate `register` functions.
///
/// Once all modules have registered their types, the `Registry` can be passed to either
/// `run::auth::Run` or `run::view::Run` (together with their `Setup`) to initialize game state and
/// run the simulation.
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

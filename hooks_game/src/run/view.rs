use run::common;

#[derive(Default)]
pub struct Setup {
    pub common: common::Setup,
}

impl Setup {
    pub fn new() -> Setup {
        Default::default()
    }
}

pub struct Run {
    run_common: common::Run,
}

impl Run {
    pub fn new(registry: Registry, setup: Setup) -> Run {
        Run {
            common: common::Run::new(registry, setup.common),
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

    /// Run a tick on the client side. Here, we are given the state of the world in terms of an
    /// `WorldSnapshot`, together with the game events, as received from the server.
    pub fn run_tick(
        &mut self,
        tick_num: TickNum,
        tick_data: &tick::Data<game::EntitySnapshot>,
        input: &PlayerInput,
    ) -> Result<Vec<Box<Event>>, repl::Error> {
        profile!("run");

        // First run the game events we received from the server
        self.run_common.run_pre_tick(tick_data.events)?;

        // Not necessarily every tick we receive from the server also contains a snapshot
        if let Some(ref snapshot) = tick_data.snapshot {
            profile!("load");

            // By now we are up-to-date regarding the player list, so we can create new entities.
            // It is important that we do this *after* handling `tick_data.events`, since we might
            // create an entity for a newly joined player here (and we keep track of each player's
            // main entity).
            repl::entity::view::create_new_entities(&mut state.world, snapshot)?;

            // Snap entities to their state in the new tick
            let mut sys = game::LoadSnapshotSys {
                snapshot,
                exclude_player: if self.predict_log.is_some() {
                    Some(self.my_player_id)
                } else {
                    None
                },
                only_player: None,
            };
            sys.run_now(&state.world.res);
        }

        // TODO: Put prediction here again

        self.common.run_post_tick()
    }
}

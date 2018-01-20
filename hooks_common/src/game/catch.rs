pub mod auth {
    use nalgebra::Point2;
    use rand::{self, Rng};
    use specs::World;

    use defs::GameInfo;
    use physics::Position;
    use registry::Registry;
    use repl;

    pub fn register(reg: &mut Registry) {
        reg.pre_tick_fn(pre_tick);
    }

    fn pre_tick(world: &mut World) -> Result<(), repl::Error> {
        let player_entity_class = world
            .read_resource::<GameInfo>()
            .player_entity_class
            .clone();
        let players = world.read_resource::<repl::player::Players>().clone();

        for (&player_id, &(ref _info, ref entity)) in &players.0 {
            if entity.is_none() {
                let mut rng = rand::thread_rng();

                // TODO: Spawn points here
                let pos = Point2::new(rng.next_f32() * 100.00, rng.next_f32() * 100.0);

                repl::entity::auth::create(world, player_id, &player_entity_class, |builder| {
                    builder.with(Position { pos })
                });
            }
        }

        Ok(())
    }
}

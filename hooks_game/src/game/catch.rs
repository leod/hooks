pub mod auth {
    use nalgebra::Point2;
    use rand::{self, Rng};
    use specs::prelude::World;

    use defs::GameInfo;
    use game::entity;
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
        assert!(player_entity_class == "player");

        let players = world.read_resource::<repl::player::Players>().clone();

        for (&player_id, player) in players.iter() {
            if player.entity.is_none() {
                let mut rng = rand::thread_rng();

                // TODO: Spawn points here
                let pos = Point2::new(
                    rng.next_f32() * 200.00 - 100.0,
                    rng.next_f32() * 200.0 - 100.0,
                );
                entity::player::auth::create(world, player_id, pos);
            }
        }

        Ok(())
    }
}

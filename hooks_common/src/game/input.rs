pub mod auth {
    use specs::prelude::{Join, World};

    use defs::{PlayerId, PlayerInput};
    use physics::{self, Update};
    use repl;
    use repl::player::Players;
    use game::entity::player;

    pub fn run_player_input(
        world: &mut World,
        physics_runner: &mut physics::sim::Runner,
        player_id: PlayerId,
        input: &PlayerInput,
    ) -> Result<(), repl::Error> {
        // TODO: We need to be careful and limit the number of inputs that may be applied in one
        //       tick. Currently, it is possible to explode the simulation by lagging the client
        //       and assumably applying too many inputs at once.

        let player_entity = {
            let players = world.read_resource::<Players>();
            players.0.get(&player_id).unwrap().entity
        };

        if let Some(player_entity) = player_entity {
            player::run_input(world, player_entity, input)?;
        }

        // Simulate only this player's entities
        {
            let repl_id = world.read::<repl::Id>();
            let mut update = world.write();

            update.clear();

            for (entity, repl_id) in (&*world.entities(), &repl_id).join() {
                if (repl_id.0).0 == player_id {
                    update.insert(entity, Update);
                }
            }
        }

        physics_runner.run(world);

        Ok(())
    }
}

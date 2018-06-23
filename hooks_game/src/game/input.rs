pub mod auth {
    use specs::prelude::{Join, World};

    use defs::{PlayerId, PlayerInput};
    use game::entity::player;
    use physics::{self, Update};
    use repl;
    use repl::player::Players;

    pub fn run_player_input(
        world: &mut World,
        physics_runner: &mut physics::sim::Runner,
        inputs: &[(PlayerId, PlayerInput)],
    ) -> Result<(), repl::Error> {
        // Take only the input of those players that currently control an entity
        let inputs_with_entity = {
            let players = world.read_resource::<Players>();

            let mut inputs_with_entity = Vec::new();
            for &(id, ref input) in inputs {
                if let Some(entity) = players.try_get(id)?.entity {
                    inputs_with_entity.push((id, input.clone(), entity));
                }
            }

            inputs_with_entity
        };

        // Simulate only these players' entities
        {
            let repl_id = world.read_storage::<repl::Id>();
            let mut update = world.write_storage();

            update.clear();

            // TODO: Maintain a separate list of repl entities for each player?
            for &(player_id, _, _) in &inputs_with_entity {
                for (entity, repl_id) in (&*world.entities(), &repl_id).join() {
                    if (repl_id.0).0 == player_id {
                        update.insert(entity, Update);
                    }
                }
            }
        }

        player::run_input(world, &inputs_with_entity)?;

        physics_runner.run(world);
        physics_runner.run_interaction_events(world)?;

        player::run_input_post_sim(world, &inputs_with_entity)?;

        Ok(())
    }
}

pub mod auth {
    use specs::World;

    use defs::{PlayerId, PlayerInput};
    use physics;
    use repl::player::Players;
    use game::entity::player;

    pub fn run_player_input(world: &mut World, player_id: PlayerId, input: &PlayerInput) {
        let entity = {
            let players = world.read_resource::<Players>();
            players.0.get(&player_id).unwrap().entity
        };

        if let Some(entity) = entity {
            player::run_input(world, entity, input);
        }

        // TODO: We need to be careful and limit the number of inputs that may be applied in one
        //       tick. Currently, it is possible to explode the simulation by lagging the client
        //       and assumably applying too many inputs at once.
        // TODO: Physics simulation should run *only* for player-owned entities every time that
        //       input is given.
        physics::sim::run(world);
    }
}

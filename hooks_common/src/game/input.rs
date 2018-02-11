pub mod auth {
    use specs::World;

    use defs::{PlayerId, PlayerInput};
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

        // TODO: Player-owned entities should move *only* in `run_player_input`. This should make
        //       prediction easier. However, it means that we need to apply the physics simulation
        //       separately for each player! Need to think about how best to do this.
    }
}

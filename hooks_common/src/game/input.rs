pub mod auth {
    use nalgebra::{Rotation2, Vector2};
    use shred::SystemData;
    use specs::{Fetch, World, WriteStorage};

    use defs::{PlayerId, PlayerInput};
    use physics::{Orientation, Velocity};
    use repl::player::Players;
    use game::entity::player;

    #[derive(SystemData)]
    struct Data<'a> {
        players: Fetch<'a, Players>,
        velocity: WriteStorage<'a, Velocity>,
        orientation: WriteStorage<'a, Orientation>,
    }

    pub const MOVE_SPEED: f32 = 100.0;

    // TODO: Player-owned entities should move *only* in `run_player_input`. This should make
    //       prediction easier. However, it means that we need to apply the physics simulation
    //       separately for each player! Need to think about how best to do this.

    // TODO: Consider running player input in some system, where player entities are tagged with a
    //       `PlayerInput` component somehow. This means not having to do all this unwrapping here
    //       and in e.g. `entity::player::shoot`.

    pub fn run_player_input(world: &mut World, player_id: PlayerId, input: &PlayerInput) {
        let entity = {
            let data = Data::fetch(&world.res, 0);
            data.players.0.get(&player_id).unwrap().entity
        };

        if let Some(entity) = entity {
            {
                let mut data = Data::fetch(&world.res, 0);

                if input.rot_angle != data.orientation.get(entity).unwrap().0 {
                    data.orientation.get_mut(entity).unwrap().0 = input.rot_angle;
                }

                let orientation = data.orientation.get(entity).unwrap().0;
                let forward = Rotation2::new(orientation).matrix() * Vector2::new(1.0, 0.0);

                //let mut position = data.position.get(entity).unwrap().0;
                //let mut changed = false;

                let velocity = data.velocity.get_mut(entity).unwrap();

                if input.move_forward {
                    //position += forward * MOVE_SPEED * data.game_info.tick_duration_secs() as f32;
                    //changed = true;
                    velocity.0 = forward * MOVE_SPEED;
                } else if input.move_backward {
                    //position -= forward * MOVE_SPEED * data.game_info.tick_duration_secs() as f32;
                    //changed = true;
                    velocity.0 = -forward * MOVE_SPEED;
                } else {
                    velocity.0 = Vector2::new(0.0, 0.0);
                }

                //if changed {
                //data.position.get_mut(entity).unwrap().0 = position;
                //}
            }

            if input.shoot_one {
                player::shoot(world, entity);
            }
        }
    }
}

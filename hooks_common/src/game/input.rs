pub mod auth {
    use nalgebra::{Rotation2, Vector2};
    use shred::SystemData;
    use specs::{Fetch, World, WriteStorage};

    use defs::{GameInfo, PlayerId, PlayerInput};
    use physics::{Orientation, Position};
    use repl::player::Players;

    #[derive(SystemData)]
    struct Data<'a> {
        game_info: Fetch<'a, GameInfo>,
        players: Fetch<'a, Players>,
        position: WriteStorage<'a, Position>,
        orientation: WriteStorage<'a, Orientation>,
    }

    pub const MOVE_SPEED: f32 = 100.0;

    pub fn run_player_input(world: &mut World, player_id: PlayerId, input: &PlayerInput) {
        // TODO: Need to decide if players should move here immediately, or if player input should
        //       only affect e.g. velocity or acceleration for a simultaneous physics tick.

        let mut data = Data::fetch(&world.res, 0);

        let entity = data.players.0.get(&player_id).unwrap().1;

        if let Some(entity) = entity {
            if input.rot_angle != data.orientation.get(entity).unwrap().0 {
                data.orientation.get_mut(entity).unwrap().0 = input.rot_angle;
            }

            let orientation = data.orientation.get(entity).unwrap().0;
            let forward = Rotation2::new(orientation).matrix() * Vector2::new(1.0, 0.0);

            let mut position = data.position.get(entity).unwrap().0;
            let mut changed = false;

            if input.move_forward {
                position += forward * MOVE_SPEED * data.game_info.tick_duration_secs() as f32;
                changed = true;
            }

            if input.move_backward {
                position -= forward * MOVE_SPEED * data.game_info.tick_duration_secs() as f32;
                changed = true;
            }

            if changed {
                data.position.get_mut(entity).unwrap().0 = position;
            }
        }
    }
}

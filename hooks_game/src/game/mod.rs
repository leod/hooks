use std::io::Cursor;
use std::time::Duration;

use bit_manager::BitReader;
use rand::{self, Rng};
use specs::World;

use hooks_util::debug;
use hooks_util::profile;
use hooks_util::stats;
use hooks_util::timer::{self, Timer};
use hooks_common::{self, event, game, GameInfo, PlayerId, PlayerInput, TickNum};
use hooks_common::net::protocol::ClientGameMsg;
use hooks_common::registry::Registry;
use hooks_common::repl::{self, tick};

use client::{self, Client};

#[derive(Debug)]
pub enum Error {
    Client(client::Error),
    Tick(tick::Error),
    Repl(repl::Error),
}

impl From<client::Error> for Error {
    fn from(error: client::Error) -> Error {
        Error::Client(error)
    }
}

impl From<tick::Error> for Error {
    fn from(error: tick::Error) -> Error {
        Error::Tick(error)
    }
}

impl From<repl::Error> for Error {
    fn from(error: repl::Error) -> Error {
        Error::Repl(error)
    }
}

pub struct Game {
    game_info: GameInfo,

    /// How many ticks we want to lag behind the server, so that we can interpolate.
    target_lag_ticks: TickNum,

    my_player_id: PlayerId,

    /// The complete state of the game.
    state: game::State,

    /// Recent ticks we have received
    tick_history: tick::History<game::EntitySnapshot>,

    /// Timer to start the next tick.
    tick_timer: Timer,

    ///
    recv_tick_timer: Timer,

    /// Number of last started tick.
    last_tick: Option<TickNum>,

    /// Newest tick of which we know that the server knows that we have received it.
    server_recv_ack_tick: Option<TickNum>,

    /// Estimated ping
    ping: f32,
}

pub fn register(reg: &mut Registry, game_info: &GameInfo) {
    hooks_common::view::register(reg, game_info);
}

#[derive(Debug)]
pub enum Event {
    Disconnected,
    TickStarted(Vec<Box<event::Event>>),
}

impl Game {
    pub fn new(reg: Registry, my_player_id: PlayerId, game_info: &GameInfo) -> Game {
        let mut state = game::State::from_registry(reg);
        game::init::view::create_state(&mut state.world);

        let tick_history = tick::History::new(state.event_reg.clone());

        Game {
            game_info: game_info.clone(),
            target_lag_ticks: 2 * game_info.ticks_per_snapshot,
            my_player_id,
            state,
            tick_history,
            tick_timer: Timer::new(game_info.tick_duration()),
            recv_tick_timer: Timer::new(
                game_info
                    .tick_duration()
                    .checked_mul(game_info.ticks_per_snapshot)
                    .unwrap(),
            ),
            last_tick: None,
            server_recv_ack_tick: None,
            ping: 5.0,
        }
    }

    pub fn world(&self) -> &World {
        &self.state.world
    }

    pub fn world_mut(&mut self) -> &mut World {
        &mut self.state.world
    }

    fn on_received_tick(&mut self, client: &mut Client, data: Vec<u8>) -> Result<(), Error> {
        let mut reader = BitReader::new(Cursor::new(data));

        let entity_classes = self.state.world.read_resource::<game::EntityClasses>();
        let tick_nums = self.tick_history
            .delta_read_tick(&entity_classes, &mut reader)?;

        if let Some((old_tick_num, new_tick_num)) = tick_nums {
            //debug!("New tick {} w.r.t. {:?}", new_tick_num, old_tick_num);
            assert!(self.tick_history.max_num() == Some(new_tick_num));

            let timer_error = timer::duration_to_secs(self.recv_tick_timer.accum()) -
                self.game_info.ticks_per_snapshot as f32 *
                    timer::duration_to_secs(self.game_info.tick_duration());
            stats::record("recv timer error", timer_error);

            self.recv_tick_timer.reset();

            if rand::thread_rng().gen() {
                // TMP: For testing delta encoding/decoding!
                let reply = ClientGameMsg::ReceivedTick(new_tick_num);
                client.send_game(reply)?;
            }

            if let Some(old_tick_num) = old_tick_num {
                // The fact that we have received a new delta encoded tick means that
                // the server knows that we have the tick w.r.t. which it was encoded.
                self.server_recv_ack_tick = Some(old_tick_num);
            }
        }

        Ok(())
    }

    fn start_tick(
        &mut self,
        client: &mut Client,
        player_input: &PlayerInput,
        tick: TickNum,
    ) -> Result<Vec<Box<event::Event>>, Error> {
        profile!("tick");

        if let Some(last_tick) = self.last_tick {
            assert!(tick > last_tick);
        }

        self.last_tick = Some(tick);

        // Inform the server
        client.send_game(ClientGameMsg::StartedTick(tick, player_input.clone()))?;

        let tick_data = self.tick_history.get(tick).unwrap();

        let events = self.state.run_tick_view(tick_data)?;
        Ok(events)
    }

    pub fn update(
        &mut self,
        client: &mut Client,
        player_input: &PlayerInput,
        delta: Duration,
    ) -> Result<Option<Event>, Error> {
        profile!("update game");

        // Handle network events
        {
            profile!("service");

            while let Some(event) = client.service()? {
                match event {
                    client::Event::Disconnected => {
                        return Ok(Some(Event::Disconnected));
                    }
                    client::Event::ServerGameMsg(data) => {
                        self.on_received_tick(client, data)?;
                    }
                }
            }
        }

        // Remove ticks from our history that:
        // 1. We know for sure will not be used by the server as a reference for delta encoding.
        // 2. We have already started.
        match (self.last_tick, self.server_recv_ack_tick) {
            (Some(last_tick), Some(server_recv_ack_tick)) => {
                self.tick_history
                    .prune_older_ticks(server_recv_ack_tick.min(last_tick));
            }
            _ => {}
        }

        if let (Some(min_tick), Some(max_tick)) =
            (self.tick_history.min_num(), self.tick_history.max_num())
        {
            if let Some(last_tick) = self.last_tick {
                let tick_duration = self.game_info.tick_duration_secs();
                let cur_time = last_tick as f32 * tick_duration + self.tick_timer.accum_secs();
                let recv_tick_time =
                    max_tick as f32 * tick_duration + self.recv_tick_timer.accum_secs();
                let target_lag_time = self.target_lag_ticks as f32 * tick_duration;
                let cur_lag_time = recv_tick_time - cur_time;
                let lag_time_error = target_lag_time - cur_lag_time;

                let warp_thresh = 0.01; // 10ms
                let max_warp = 2.0;
                let min_warp = 0.5;

                /*let warp_factor = if lag_time_error < warp_thresh {
                    1.5
                } else if lag_time_error > -warp_thresh {
                    1.0 / 1.5
                } else {
                    1.0
                };*/

                let warp_factor = 0.5 + (2.0 - 0.5) / (1.0 + 2.0 * (lag_time_error / 0.05).exp());

                self.recv_tick_timer += delta;
                self.tick_timer +=
                    timer::secs_to_duration(timer::duration_to_secs(delta) * warp_factor);

                // For debugging, record some values
                stats::record("time lag target", target_lag_time);
                stats::record("time lag current", cur_lag_time);
                stats::record("time lag error", lag_time_error);
                stats::record("time warp factor", warp_factor);
                stats::record("lag ticks target", self.target_lag_ticks as f32);
                stats::record("lag ticks current", (max_tick - last_tick) as f32);
                stats::record(
                    "lag ticks error",
                    (max_tick - last_tick) as f32 - self.target_lag_ticks as f32,
                );

                // Start ticks
                if last_tick < max_tick && self.tick_timer.trigger() {
                    // NOTE: `tick::History` always makes sure that there are no gaps in the stored
                    //       tick nums. Even if we have not received a snapshot for some tick, it
                    //       will be created (including its events) when we receive a newer tick.
                    let next_tick = last_tick + 1;

                    let events = self.start_tick(client, player_input, next_tick)?;
                    Ok(Some(Event::TickStarted(events)))
                } else {
                    Ok(None)
                }
            } else {
                // Start our first tick
                let events = self.start_tick(client, player_input, min_tick)?;
                Ok(Some(Event::TickStarted(events)))
            }
        } else {
            // We have not received our first tick yet
            assert!(self.last_tick.is_none());

            Ok(None)
        }
    }
}

impl debug::Inspect for Game {
    fn inspect(&self) -> debug::Vars {
        debug::Vars::Node(vec![
            (
                "min tick".to_string(),
                self.tick_history.min_num().inspect(),
            ),
            (
                "max tick".to_string(),
                self.tick_history.max_num().inspect(),
            ),
            ("last tick".to_string(), self.last_tick.inspect()),
            (
                "server recv ack tick".to_string(),
                self.server_recv_ack_tick.inspect(),
            ),
        ])
    }
}

use std::io::Cursor;
use std::time::Duration;

use bit_manager::BitReader;
use rand::{self, Rng};
use specs::{RunNow, World};

use hooks_util::debug;
use hooks_util::profile;
use hooks_util::stats;
use hooks_util::timer::{self, Timer};
use hooks_common::{self, event, game, GameInfo, PlayerId, PlayerInput, TickNum};
use hooks_common::net::protocol::ClientGameMsg;
use hooks_common::registry::Registry;
use hooks_common::physics::{Orientation, Position};
use hooks_common::repl::{self, interp, tick};

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

    /// When do we expect to receive the next snapshot?
    recv_snapshot_timer: Timer,

    /// Number of last started tick.
    last_tick: Option<TickNum>,

    /// Number of last started tick that also contained a snapshot. This is used for interpolation.
    last_snapshot_tick: Option<TickNum>,

    /// Number of the tick we are currently interpolating into. If given, must be larger than
    /// `last_tick`.
    interp_tick: Option<TickNum>,

    /// Newest tick of which we know that the server knows that we have received it.
    server_recv_ack_tick: Option<TickNum>,

    /// Estimated ping
    ping: f32,
}

pub fn register(reg: &mut Registry, game_info: &GameInfo) {
    hooks_common::view::register(reg, game_info);

    reg.component::<interp::State<Position>>();
    reg.component::<interp::State<Orientation>>();
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
            recv_snapshot_timer: Timer::new(
                game_info
                    .tick_duration()
                    .checked_mul(game_info.ticks_per_snapshot)
                    .unwrap(),
            ),
            last_tick: None,
            last_snapshot_tick: None,
            interp_tick: None,
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

            let timer_error = timer::duration_to_secs(self.recv_snapshot_timer.accum()) -
                timer::duration_to_secs(self.recv_snapshot_timer.period());
            stats::record("recv timer error", timer_error);

            self.recv_snapshot_timer.reset();

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
        if let Some(last_snapshot_tick) = self.last_snapshot_tick {
            assert!(tick > last_snapshot_tick);
        }

        self.last_tick = Some(tick);

        // Inform the server
        client.send_game(ClientGameMsg::StartedTick(tick, player_input.clone()))?;

        let tick_data = self.tick_history.get(tick).unwrap();

        if tick_data.snapshot.is_some() {
            // This tick contains a snapshot, so remember that we want to use it as the
            // basis for interpolation from now on
            self.last_snapshot_tick = Some(tick);
        }

        let events = self.state.run_tick_view(tick_data)?;
        Ok(events)
    }

    fn next_interp_tick(&self) -> Option<TickNum> {
        self.last_snapshot_tick
            .map(|last_snapshot_tick| {
                // If we have a started tick, the history will contain at least one element,
                // so we can unwrap here.
                let max_tick = self.tick_history.max_num().unwrap();

                // Find the next tick for which we received a snapshot we can interpolate into
                (last_snapshot_tick + 1..max_tick).find(|&tick| {
                    self.tick_history
                        .get(tick)
                        .map(|data| data.snapshot.is_some())
                        .unwrap_or(false)
                })
            })
            .and_then(|x| x)
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
        match (self.last_snapshot_tick, self.server_recv_ack_tick) {
            (Some(last_snapshot_tick), Some(server_recv_ack_tick)) => {
                self.tick_history
                    .prune_older_ticks(server_recv_ack_tick.min(last_snapshot_tick));
            }
            _ => {}
        }

        // Update tick timing and start next tick if necessary
        if let (Some(min_tick), Some(max_tick)) =
            (self.tick_history.min_num(), self.tick_history.max_num())
        {
            if let Some(last_tick) = self.last_tick {
                let tick_duration = self.game_info.tick_duration_secs();
                let cur_time = last_tick as f32 * tick_duration + self.tick_timer.accum_secs();
                let recv_snapshot_time =
                    max_tick as f32 * tick_duration + self.recv_snapshot_timer.accum_secs();
                let target_lag_time = self.target_lag_ticks as f32 * tick_duration;
                let cur_lag_time = recv_snapshot_time - cur_time;
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

                self.recv_snapshot_timer += delta;
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

    pub fn interpolate(&mut self) {
        // Interpolate into the next tick where we have a snapshot
        let next_interp_tick = self.next_interp_tick();

        if let Some(next_interp_tick) = next_interp_tick {
            // Can unwrap, since otherwise next_interp_tick would be none
            let last_tick = self.last_tick.unwrap();
            let last_snapshot_tick = self.last_snapshot_tick.unwrap();

            // Have we already loaded our interpolation state?
            let loaded = if let Some(cur_interp_tick) = self.interp_tick {
                assert!(next_interp_tick >= cur_interp_tick);
                next_interp_tick == cur_interp_tick
            } else {
                // First time interpolating in this game
                false
            };

            if !loaded {
                // State of next_interp_tick has not been loaded yet
                let last_snapshot = self.tick_history
                    .get(last_snapshot_tick)
                    .unwrap()
                    .snapshot
                    .as_ref()
                    .unwrap();
                let next_snapshot = self.tick_history
                    .get(next_interp_tick)
                    .unwrap()
                    .snapshot
                    .as_ref()
                    .unwrap();

                let mut sys = interp::LoadStateSys::<game::EntitySnapshot, Position>::new(
                    &last_snapshot,
                    &next_snapshot,
                );
                sys.run_now(&self.state.world.res);

                let mut sys = interp::LoadStateSys::<game::EntitySnapshot, Orientation>::new(
                    &last_snapshot,
                    &next_snapshot,
                );
                sys.run_now(&self.state.world.res);

                self.interp_tick = Some(next_interp_tick);
            }

            // Interpolate based on the progress between `last_snapshot_tick` and
            // `next_interp_tick`.
            assert!(last_snapshot_tick < next_interp_tick);
            assert!(last_snapshot_tick <= last_tick);
            assert!(last_tick < next_interp_tick);

            let delta_ticks = next_interp_tick - last_snapshot_tick;
            let done_ticks = last_tick - last_snapshot_tick;

            let interp_t = (done_ticks as f32 + self.tick_timer.progress()) / delta_ticks as f32;
            //stats::record("interp time", interp_t);
            //debug!("{}", interp_t);

            let mut sys = interp::InterpSys::<Position>::new(interp_t);
            sys.run_now(&self.state.world.res);

            let mut sys = interp::InterpSys::<Orientation>::new(interp_t);
            sys.run_now(&self.state.world.res);
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
            ("interp_tick".to_string(), self.interp_tick.inspect()),
            ("last tick".to_string(), self.last_tick.inspect()),
            (
                "server recv ack tick".to_string(),
                self.server_recv_ack_tick.inspect(),
            ),
        ])
    }
}

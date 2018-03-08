use std::collections::BTreeMap;
use std::collections::Bound::{Excluded, Included, Unbounded};
use std::collections::btree_map;

use bit_manager::{self, BitRead, BitWrite};

use defs::{TickDeltaNum, TickNum, INVALID_PLAYER_ID, NO_DELTA_TICK};
use event::{self, Event};
use repl::{entity, player};
use repl::snapshot::{self, EntityClasses, EntitySnapshot, WorldSnapshot};

pub struct Data<T: EntitySnapshot> {
    /// Game events that happened in this tick.
    pub events: Vec<Box<Event>>,

    /// State of replicated entities at the end of the tick. Note that not every tick needs to have
    /// a snapshot, meaning that the server can run ticks at a higher frequency than it sends
    /// snapshots to the clients. However, the client still receives events that happened in those
    /// intermediate ticks, together with the data of the next tick that does include a snapshot.
    pub snapshot: Option<WorldSnapshot<T>>,

    /// The last of our player input that has been run in this tick, if any.
    pub last_input_num: Option<TickNum>,
}

#[derive(Debug)]
pub enum Error {
    ReceivedInvalidTick(TickNum, String),
    Event(event::Error),
    Snapshot(snapshot::Error),
    BitManager(bit_manager::Error),
}

impl From<event::Error> for Error {
    fn from(error: event::Error) -> Error {
        Error::Event(error)
    }
}

impl From<snapshot::Error> for Error {
    fn from(error: snapshot::Error) -> Error {
        Error::Snapshot(error)
    }
}

impl From<bit_manager::Error> for Error {
    fn from(error: bit_manager::Error) -> Error {
        Error::BitManager(error)
    }
}

pub struct History<T: EntitySnapshot> {
    event_reg: event::Registry,
    ticks: BTreeMap<TickNum, Data<T>>,
}

impl<T: EntitySnapshot> History<T> {
    pub fn new(event_reg: event::Registry) -> Self {
        Self {
            event_reg: event_reg,
            ticks: BTreeMap::new(),
        }
    }

    pub fn min_num(&self) -> Option<TickNum> {
        self.ticks.keys().next().cloned()
    }

    pub fn max_num(&self) -> Option<TickNum> {
        self.ticks.keys().next_back().cloned()
    }

    pub fn len(&self) -> usize {
        self.ticks.len()
    }

    pub fn push_tick(&mut self, num: TickNum, data: Data<T>) {
        assert!(!self.ticks.contains_key(&num));

        // No gaps in recording snapshots on the server
        if let Some(max_num) = self.max_num() {
            assert!(max_num + 1 == num);
        }

        self.ticks.insert(num, data);
    }

    pub fn get(&self, num: TickNum) -> Option<&Data<T>> {
        self.ticks.get(&num)
    }

    pub fn prune_older_ticks(&mut self, new_min_num: TickNum) {
        if let Some(min_num) = self.min_num() {
            let range = (Included(min_num), Excluded(new_min_num));
            let prune_nums = self.ticks
                .range(range)
                .map(|(&num, _)| num)
                .collect::<Vec<_>>();

            for num in prune_nums {
                self.ticks.remove(&num);
            }
        }

        assert!(self.min_num().is_none() || new_min_num <= self.min_num().unwrap());
    }

    /// Encode tick data w.r.t. a previous tick. Contains the changed components as well as all the
    /// events that happened inbetween.
    pub fn delta_write_tick(
        &self,
        prev_num: Option<TickNum>,
        cur_num: TickNum,
        classes: &EntityClasses<T>,
        writer: &mut event::Writer,
    ) -> Result<(), bit_manager::Error> {
        writer.write(&cur_num)?;

        // How many ticks in the past is the reference tick?
        let delta_num = if let Some(prev_num) = prev_num {
            // Reference tick must not be too far in the past
            assert!(prev_num < cur_num);
            assert!(cur_num - prev_num <= TickDeltaNum::max_value() as TickNum);

            (cur_num - prev_num) as TickDeltaNum
        } else {
            NO_DELTA_TICK
        };
        writer.write(&delta_num)?;

        let cur_data = &self.ticks[&cur_num];

        writer.write(&cur_data.last_input_num)?;

        // Send events of all ticks between previous and current tick
        {
            let event_range = if let Some(prev_num) = prev_num {
                // Send all the events that happened between the reference tick and now
                (Included(prev_num), Included(cur_num))
            } else {
                // No delta snapshot encoding
                if self.min_num().is_some() {
                    // Although we have sent ticks to the client, it has not acknowledged any of them
                    // yet. Therefore, we have to resend all events from the start.
                    (Unbounded, Included(cur_num))
                } else {
                    // No ticks sent yet, so there are no events we could resend
                    (Excluded(cur_num), Included(cur_num))
                }
            };

            // Sanity check: should have tick data for every tick inbetween
            assert!(
                self.ticks
                    .range(event_range)
                    .zip(self.ticks.range(event_range).skip(1))
                    .all(|((num, _), (&next_num, _))| num + 1 == next_num)
            );

            // Write tick events backwards
            for (&num, data) in self.ticks.range(event_range).rev() {
                assert!(num <= cur_num);
                assert!(cur_num - num <= TickDeltaNum::max_value() as TickNum);

                writer.write_bit(true)?;
                self.write_events(&data.events, writer)?;
            }

            // End of event stream
            writer.write_bit(false)?;
        }

        // Send delta world snapshot
        {
            let empty_snapshot = WorldSnapshot::new();
            let prev_snapshot = if let Some(prev_num) = prev_num {
                // On the server, we assume that all ticks have a snapshot
                &self.ticks[&prev_num].snapshot.as_ref().unwrap()
            } else {
                // Delta serialize with respect to an empty snapshot
                &empty_snapshot
            };

            let cur_snapshot = cur_data.snapshot.as_ref().unwrap();

            // Write snapshot delta
            prev_snapshot.delta_write(
                cur_snapshot,
                classes,
                INVALID_PLAYER_ID, // TODO
                writer,
            )?;
        }

        Ok(())
    }

    /// Decode tick data. If the tick was new to us, returns a pair of tick nums, where the first
    /// element is the reference tick num and the second element is the new tick num.
    pub fn delta_read_tick(
        &mut self,
        classes: &EntityClasses<T>,
        reader: &mut event::Reader,
    ) -> Result<Option<(Option<TickNum>, TickNum)>, Error> {
        let cur_num = reader.read::<TickNum>()?;

        if self.max_num().is_some() && cur_num < self.max_num().unwrap() {
            // The server will always send tick data in order, so here we can assume that we
            // have received packets out of order and ignore this tick data.
            return Ok(None);
        }
        assert!(!self.ticks.contains_key(&cur_num));

        let delta_num = reader.read::<TickDeltaNum>()?;

        let prev_num = if delta_num != NO_DELTA_TICK {
            // Sanity checks for the reference tick
            if delta_num as TickNum > cur_num {
                return Err(Error::ReceivedInvalidTick(
                    cur_num,
                    format!("tick reference is {} ticks in the past", delta_num),
                ));
            }

            let prev_num = cur_num - (delta_num as TickNum);

            if !self.ticks.contains_key(&prev_num) {
                return Err(Error::ReceivedInvalidTick(
                    cur_num,
                    format!("we don't have tick data for reference {}", prev_num),
                ));
            }

            Some(prev_num)
        } else {
            // No delta tick
            None
        };

        let last_input_num = reader.read::<Option<TickNum>>()?;

        // Loop for reading events backwards
        let mut event_tick_num = cur_num + 1;

        while reader.read_bit()? {
            if event_tick_num == 0 {
                return Err(Error::ReceivedInvalidTick(
                    cur_num,
                    "received too many event lists".to_string(),
                ));
            }

            event_tick_num -= 1;

            let events = self.read_events(reader)?;

            // It is possible that we receive events of the same tick more than once. This can
            // happen if the server sends us multiple ticks as a delta with reference to the
            // same previous tick, because it has not received our acknowledgment. If we receive
            // the same tick twice, all we have to do is ignore it.
            if let btree_map::Entry::Vacant(entry) = self.ticks.entry(event_tick_num) {
                // For non-existent intermediate ticks, we only have the events, but no world snapshot
                let prev_data = Data {
                    events: events,
                    snapshot: None,
                    last_input_num: None,
                };

                entry.insert(prev_data);
            }
        }

        // Sanity checks
        if prev_num.is_some() && event_tick_num != prev_num.unwrap() {
            return Err(Error::ReceivedInvalidTick(
                cur_num,
                format!(
                    "`event_tick_num` {} should be equal to `prev_num` {} after event loop",
                    event_tick_num,
                    prev_num.unwrap()
                ),
            ));
        }

        // Finally, we are done with events and can delta read the snapshot
        let (_new_entities, mut cur_snapshot) = {
            let empty_snapshot = WorldSnapshot::new();
            let prev_snapshot = if let Some(prev_num) = prev_num {
                // We have an entry for `prev_num` due to the loop for reading events
                let prev_data = &self.ticks[&prev_num];

                match prev_data.snapshot.as_ref() {
                    Some(prev_snapshot) => prev_snapshot,
                    None => {
                        return Err(Error::ReceivedInvalidTick(
                            cur_num,
                            format!("don't have snapshot data for delta reference {}", prev_num),
                        ));
                    }
                }
            } else {
                // No delta snapshot. Assume emtpy snapshot for delta reading
                &empty_snapshot
            };

            prev_snapshot.delta_read(classes, reader)?
        };

        // In case we receive an `entity::RemoveOrder`, we have to make sure not to carry around
        // that entity's snapshot anymore --- otherwise, the local world snapshots could grow
        // indefinitely. I didn't consider this at first, which led to removed entities immediately
        // being recreated on the next tick. Putting this special case here feels like a bit of a
        // hack, I need to think about the repercussions.
        for (_num, tick_data) in self.ticks
            .range((Included(event_tick_num), Included(cur_num)))
        {
            for event in &tick_data.events {
                match_event!(event:
                    entity::RemoveOrder => {
                        debug!("Tick remove {:?}", event.0);
                        cur_snapshot.0.remove(&event.0);
                    },
                    player::LeftEvent => {
                        // Player entities are removed implicitly on disconnect, so we have to do
                        // this here as well...
                        let ids = cur_snapshot.0.iter()
                            .filter_map(|(&id, &(ref entity, ref _snapshot))| {
                                if id.0 == event.id {
                                    Some(id)
                                } else {
                                    None
                                }
                            }).collect::<Vec<_>>();

                        for id in &ids {
                            debug!("Tick remove {:?}", id);
                            cur_snapshot.0.remove(id);
                        }
                    },
                );
            }
        }

        // Finally, add the new snapshot in the history
        // NOTE: Here, the tick data entry has already been created by the loop for reading events
        //       of intermediate ticks. The intermediate ticks do not have a snapshot.
        let cur_data = self.ticks.get_mut(&cur_num).unwrap();
        cur_data.snapshot = Some(cur_snapshot);
        cur_data.last_input_num = last_input_num;

        Ok(Some((prev_num, cur_num)))
    }

    fn write_events(
        &self,
        events: &[Box<Event>],
        writer: &mut event::Writer,
    ) -> Result<(), bit_manager::Error> {
        writer.write_bit(!events.is_empty())?;
        if !events.is_empty() {
            // TODO: u16 should be enough here right?
            //       How about using variable-length integer encodings for such things?
            writer.write(&(events.len() as u32))?;
            for event in events {
                self.event_reg.write(&**event, writer)?;
            }
        }

        Ok(())
    }

    fn read_events(&self, reader: &mut event::Reader) -> Result<Vec<Box<Event>>, Error> {
        let events = if reader.read_bit()? {
            // TODO: u16 should be enough here right?
            let len = reader.read::<u32>()?;
            let mut events = Vec::new();
            for _ in 0..len {
                let event = self.event_reg.read(reader)?;
                events.push(event);
            }
            events
        } else {
            Vec::new()
        };

        Ok(events)
    }
}

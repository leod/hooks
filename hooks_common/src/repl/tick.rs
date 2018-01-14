use std::collections::BTreeMap;
use std::collections::Bound::{Excluded, Included};
use std::collections::btree_map;

use bit_manager::{self, BitRead, BitWrite};

use defs::{TickNum, INVALID_PLAYER_ID};
use event::{self, Event};
use repl::snapshot::{self, EntityClasses, EntitySnapshot, WorldSnapshot};

#[derive(Debug)]
pub enum Error {
    ReceivedInvalidTick(String),
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

pub struct Data<T: EntitySnapshot> {
    pub events: Vec<Box<Event>>,
    pub snapshot: Option<WorldSnapshot<T>>,
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

    pub fn push_tick(&mut self, num: TickNum, data: Data<T>) -> TickNum {
        // No gaps in recording snapshots on the server
        if let Some(max_num) = self.max_num() {
            assert!(max_num + 1 == num);
        }

        self.ticks.insert(num, data);
        num
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

    pub fn delta_write_tick(
        &self,
        prev_num: Option<TickNum>,
        cur_num: TickNum,
        classes: &EntityClasses<T>,
        writer: &mut event::Writer,
    ) -> Result<(), bit_manager::Error> {
        writer.write(&cur_num)?;

        let cur_data = &self.ticks[&cur_num];
        self.write_events(&cur_data.events, writer)?;

        if let Some(prev_num) = prev_num {
            // Send all the events that happened inbetween
            let range = (Included(prev_num), Excluded(cur_num));

            // Sanity check: we should have data for every tick inbetween
            assert!(
                self.ticks
                    .range(range)
                    .map(|(num, _)| *num)
                    .eq(prev_num..cur_num)
            );

            let iter = self.ticks
                .range(range)
                .map(|(num, data)| (num, &data.events))
                .rev();

            for (_num, events) in iter {
                writer.write_bit(true)?;
                self.write_events(events, writer)?;
            }
        }

        // End of event stream
        writer.write_bit(false)?;

        let empty_data = Data {
            events: Vec::new(),
            snapshot: Some(WorldSnapshot::new()),
        };
        let prev_data = if let Some(prev_num) = prev_num {
            &self.ticks[&prev_num]
        } else {
            // Delta serialize with respect to an empty snapshot
            &empty_data
        };

        // On the server, we assume that all ticks have a snapshot
        let prev_snapshot = prev_data.snapshot.as_ref().unwrap();
        let cur_snapshot = cur_data.snapshot.as_ref().unwrap();

        // Write snapshot delta
        prev_snapshot.delta_write(
            cur_snapshot,
            classes,
            INVALID_PLAYER_ID, // TODO
            writer,
        )?;

        Ok(())
    }

    /// Decode tick data. If the tick was new to use, returns a pair of tick nums, where the first
    /// element is the reference tick num and the second element is the new tick num.
    pub fn delta_read_tick(
        &mut self,
        classes: &EntityClasses<T>,
        reader: &mut event::Reader,
    ) -> Result<Option<(TickNum, TickNum)>, Error> {
        let cur_num = reader.read::<TickNum>()?;

        if self.max_num().is_some() && cur_num < self.max_num().unwrap() {
            // The server will always send tick data in order, so here we can assume that we
            // received the packets out of order and ignore this data.
            return Ok(None);
        }
        assert!(!self.ticks.contains_key(&cur_num));

        let cur_events = self.read_events(reader)?;

        // Read events of previous ticks. The following loop mirrors `delta_write_tick`. After it
        // is finished, `prev_num` contains the number of the tick reference for delta decoding.
        let mut prev_num = cur_num;

        loop {
            if !reader.read_bit()? {
                // End of event stream
                break;
            }

            if prev_num == 0 {
                return Err(Error::ReceivedInvalidTick(
                    "received too many event lists".to_string(),
                ));
            }

            prev_num -= 1;

            if self.min_num().is_none() || prev_num < self.min_num().unwrap() {
                // This should not happen, since it means we can't delta decode
                return Err(Error::ReceivedInvalidTick(
                    "`prev_num` for tick delta points beyond our front".to_string(),
                ));
            }

            let events = self.read_events(reader)?;

            // It is possible that we receive events of the same tick more than once. This can
            // happen if the server sends us multiple ticks as a delta with reference to the
            // same previous tick, because it has not received our acknowledgment. If we receive
            // the same tick twice, all we have to do is ignore it.
            if let btree_map::Entry::Vacant(entry) = self.ticks.entry(prev_num) {
                // For non-existent intermediate ticks, we only have the events, but no world snapshot
                let prev_data = Data {
                    events: events,
                    snapshot: None,
                };

                entry.insert(prev_data);
            }
        }

        // Finally, we are done with events and can delta read the snapshot
        assert!(prev_num <= cur_num);

        let (_new_entities, cur_snapshot) = {
            let empty_snapshot = WorldSnapshot::new();
            let prev_snapshot = if cur_num > prev_num {
                // We have an entry for `prev_num` due to the loop for reading events
                let prev_data = &self.ticks[&prev_num];

                match prev_data.snapshot.as_ref() {
                    Some(prev_snapshot) => prev_snapshot,
                    None => {
                        return Err(Error::ReceivedInvalidTick(
                            "don't have previous snapshot data for delta reading".to_string(),
                        ));
                    }
                }
            } else {
                // Assume emtpy snapshot for delta reading
                &empty_snapshot
            };

            prev_snapshot.delta_read(classes, reader)?
        };

        let cur_data = Data {
            events: cur_events,
            snapshot: Some(cur_snapshot),
        };

        self.ticks.insert(cur_num, cur_data);

        Ok(Some((prev_num, cur_num)))
    }

    fn write_events(
        &self,
        events: &[Box<Event>],
        writer: &mut event::Writer,
    ) -> Result<(), bit_manager::Error> {
        writer.write_bit(!events.is_empty())?;
        if !events.is_empty() {
            writer.write(&(events.len() as u32))?;
            for event in events {
                self.event_reg.write(&**event, writer)?;
            }
        }

        Ok(())
    }

    fn read_events(&self, reader: &mut event::Reader) -> Result<Vec<Box<Event>>, Error> {
        let events = if reader.read_bit()? {
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

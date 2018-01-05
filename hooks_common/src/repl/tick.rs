use std::collections::BTreeMap;
use std::collections::Bound::{Excluded, Included};

use bit_manager::{BitRead, BitWrite, Error, Result};

use defs::{GameEvent, TickNum, INVALID_PLAYER_ID};

pub use self::snapshot::{EntityClasses, EntitySnapshot, WorldSnapshot};

snapshot! {
    use physics::Position;
    use physics::Orientation;

    mod snapshot {
        position: Position,
        orientation: Orientation,
    }
}

pub struct TickData {
    events: Vec<GameEvent>,
    snapshot: Option<WorldSnapshot>,
}

pub struct TickHistory {
    ticks: BTreeMap<TickNum, TickData>,
}

impl TickHistory {
    fn write_events<W: BitWrite>(events: &[GameEvent], writer: &mut W) -> Result<()> {
        writer.write_bit(!events.is_empty())?;
        if !events.is_empty() {
            writer.write(&(events.len() as u32))?;
            for event in events {
                writer.write(event)?;
            }
        }

        Ok(())
    }

    fn read_events<R: BitRead>(reader: &mut R) -> Result<Vec<GameEvent>> {
        let events = if reader.read_bit()? {
            let len = reader.read::<u32>()?;
            let mut events = Vec::new();
            for _ in 0..len {
                let event = reader.read::<GameEvent>()?;
                events.push(event);
            }
            events
        } else {
            Vec::new()
        };

        Ok(events)
    }

    pub fn min_num(&self) -> Option<TickNum> {
        self.ticks.iter().next().map(|(&num, _)| num)
    }

    pub fn max_num(&self) -> Option<TickNum> {
        self.ticks.iter().next_back().map(|(&num, _)| num)
    }

    pub fn len(&self) -> usize {
        self.ticks.len()
    }

    pub fn push_tick(&mut self, data: TickData) -> TickNum {
        // No gaps in recording snapshots on the server
        let num = self.max_num().unwrap_or(0) + 1;
        self.ticks.insert(num, data);
        num
    }

    pub fn prune_older_ticks(&mut self, new_min_num: TickNum) {
        if let Some(min_num) = self.min_num() {
            let range = (Included(min_num), Excluded(new_min_num));
            let prune_nums = self.ticks.range(range).map(|(&num, _)| num).collect::<Vec<_>>();
            
            for num in prune_nums {
                self.ticks.remove(&num);
            }
        }

        assert!(self.min_num().is_none() || new_min_num < self.min_num().unwrap());
    }

    pub fn delta_write_tick<W: BitWrite>(
        &self,
        prev_num: Option<TickNum>,
        cur_num: TickNum,
        classes: &EntityClasses,
        writer: &mut W,
    ) -> Result<()> {
        writer.write(&cur_num)?;

        let cur_data = self.ticks.get(&cur_num).unwrap();
        Self::write_events(&cur_data.events, writer)?;

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
                Self::write_events(events, writer)?;
            }
        }

        // End of event stream
        writer.write_bit(false)?;

        let empty_data = TickData {
            events: Vec::new(),
            snapshot: Some(WorldSnapshot::new()),
        };
        let prev_data = if let Some(prev_num) = prev_num {
            self.ticks.get(&prev_num).unwrap()
        } else {
            // Delta serialize with respect to an empty snapshot
            &empty_data
        };

        // On the server, we assume that all ticks have a snapshot
        let prev_snapshot = prev_data.snapshot.as_ref().unwrap();
        let cur_snapshot = cur_data.snapshot.as_ref().unwrap();

        // Write snapshot delta
        prev_snapshot.delta_write(
            &cur_snapshot,
            classes,
            INVALID_PLAYER_ID, // TODO
            writer,
        )?;

        Ok(())
    }

    pub fn delta_read_tick<R: BitRead>(
        &mut self,
        classes: &EntityClasses,
        reader: &mut R,
    ) -> Result<Option<TickNum>> {
        let cur_num = reader.read::<TickNum>()?;

        if self.max_num().is_some() && cur_num < self.max_num().unwrap() {
            // The server will always send tick data in order, so here we can assume that we
            // received the packets out of order and ignore this data.
            return Ok(None);
        }
        assert!(!self.ticks.contains_key(&cur_num));

        let cur_events = Self::read_events(reader)?;

        // Read events of previous ticks. The following loop mirrors `delta_write_tick`. After it
        // is finished, `prev_num` contains the number of the tick reference for delta decoding.
        let mut prev_num = cur_num;

        loop {
            if !reader.read_bit()? {
                // End of event stream
                break;
            }

            if prev_num == 0 {
                return Err(Error::OtherError {
                    message: Some(String::from("received too many event lists")),
                });
            }

            prev_num -= 1;

            if self.min_num().is_none() || prev_num < self.min_num().unwrap() {
                // This should not happen, since it means we can't delta decode
                return Err(Error::OtherError {
                    message: Some(String::from(
                        "`prev_num` for tick delta points beyond our front",
                    )),
                });
            }

            let events = Self::read_events(reader)?;

            // It is possible that we receive events of the same tick more than once. This can
            // happen if the server sends us multiple ticks as a delta with reference to the
            // same previous tick, because it has not received our acknowledgment. If we receive
            // the same tick twice, all we have to do is ignore it.
            if !self.ticks.contains_key(&prev_num) {
                // For these intermediate ticks, we only have the events, but no world snapshot
                let prev_data = TickData {
                    events: events,
                    snapshot: None,
                };

                self.ticks.insert(prev_num, prev_data);
            }
        }

        // Finally, we are done with events and can delta read the snapshot
        assert!(prev_num < cur_num);

        let (_new_entities, cur_snapshot) = {
            let empty_snapshot = WorldSnapshot::new();
            let prev_snapshot = if cur_num > prev_num {
                // We have an entry for `prev_num` due to the loop for reading events
                let prev_data = self.ticks.get(&prev_num).unwrap();

                match prev_data.snapshot.as_ref() {
                    Some(prev_snapshot) => prev_snapshot,
                    None => {
                        return Err(Error::OtherError {
                            message: Some(String::from(
                                "don't have previous snapshot data for delta reading",
                            )),
                        });
                    }
                }
            } else {
                // Assume emtpy snapshot for delta reading
                &empty_snapshot
            };

            prev_snapshot.delta_read(classes, reader)?
        };

        let cur_data = TickData {
            events: cur_events,
            snapshot: Some(cur_snapshot),
        };

        self.ticks.insert(cur_num, cur_data);

        Ok(Some(cur_num))
    }
}

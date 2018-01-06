use std::any::{Any, TypeId};
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::io::{Cursor, Read, Write};
use std::u16;

use bit_manager::{BitRead, BitReader, BitWrite, BitWriter, Result};
use bit_manager::data::BitStore;

use mopa;

pub type EventTypeId = u16;

type Writer = BitWriter<Vec<u8>>;
type Reader = BitReader<Cursor<Vec<u8>>>;

pub trait Event: mopa::Any + Debug {
    fn type_id(&self) -> TypeId;
    fn write(&self, writer: &mut Writer) -> Result<()>;
}

mopafy!(Event);

impl<T: Any + Debug + BitStore> Event for T {
    fn type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }

    fn write(&self, writer: &mut Writer) -> Result<()> {
        self.write_to(writer)
    }
}

struct EventType {
    pub type_id: TypeId,
    pub read: fn(&mut Reader) -> Result<Box<Event>>,
}

fn read_event<T: Event + BitStore>(reader: &mut Reader) -> Result<Box<Event>> {
    Ok(Box::new(T::read_from(reader)?))
}

pub struct EventRegistry {
    /// Event types indexed by EventTypeId
    event_types: Vec<EventType>,

    /// Map from type id to EventTypeId
    event_type_ids: BTreeMap<TypeId, EventTypeId>,
}

impl EventRegistry {
    pub fn new() -> Self {
        Self {
            event_types: Vec::new(),
            event_type_ids: BTreeMap::new(),
        }
    }

    pub fn add<T: Event + BitStore>(&mut self) {
        assert!(
            self.event_types.len() <= u16::MAX as usize,
            "too many event types"
        );

        let type_id = TypeId::of::<T>();
        let event_type_id = self.event_types.len() as u16;

        let event_type = EventType {
            type_id: type_id,
            read: read_event::<T>,
        };

        self.event_types.push(event_type);
        self.event_type_ids.insert(type_id, event_type_id);
    }

    pub fn write(&self, event: &Box<Event>, writer: &mut Writer) -> Result<()> {
        let type_id = (*event).type_id();
        let event_type_id = self.event_type_ids.get(&type_id).unwrap();

        writer.write(event_type_id)?;
        event.write(writer)
    }

    pub fn read(&self, reader: &mut Reader) -> Result<Box<Event>> {
        let event_type_id = reader.read::<EventTypeId>()?;

        let event_type = &self.event_types[event_type_id as usize];
        (event_type.read)(reader)
    }
}

macro_rules! match_event {
    {
        $event:ident:
        $($typ:ty => $body:expr),*,
    } => {
        {
            $(
                if let Some($event) = $event.downcast_ref::<$typ>() {
                    $body
                }
            )*
        }
    };
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use bit_manager::{BitRead, BitReader, BitWrite, BitWriter};

    use super::{Event, EventRegistry};

    #[derive(Debug, BitStore)]
    struct A;

    #[derive(Debug, BitStore)]
    struct B(bool);

    #[derive(Debug, BitStore, PartialEq, Eq)]
    enum C {
        X,
        Y(i32, bool),
    }

    #[test]
    fn test_match() {
        let event: Box<Event> = Box::new(A);

        let mut n: usize = 0;
        match_event!(event:
            A => n += 1,
            A => n += 1,
            B => assert!(false),
            C => assert!(false),
        );

        assert!(n == 2);
    }

    #[test]
    fn test_write_read() {
        let mut reg = EventRegistry::new();
        reg.add::<A>();
        reg.add::<B>();
        reg.add::<C>();

        let event: Box<Event> = Box::new(C::Y(42, true));

        let data = {
            let mut writer = BitWriter::new(Vec::new());
            reg.write(&event, &mut writer).unwrap();
            writer.into_inner().unwrap()
        };

        let read_event = {
            let mut reader = BitReader::new(Cursor::new(data));
            reg.read(&mut reader).unwrap()
        };

        let mut n: usize = 0;
        match_event!(read_event:
            A => assert!(false),
            B => assert!(false),
            C => {
                assert!(*read_event == C::Y(42, true));
                n += 1;
            },
        );

        assert!(n == 1);
    }
}

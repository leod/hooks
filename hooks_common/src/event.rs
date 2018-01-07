use std::any::{self, Any};
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::io::Cursor;
use std::u16;

use bit_manager::{BitRead, BitReader, BitWrite, BitWriter, Result};
use bit_manager::data::BitStore;

use mopa;

pub type TypeIndex = u16;

pub type Writer = BitWriter<Vec<u8>>;
pub type Reader = BitReader<Cursor<Vec<u8>>>;

pub enum Class {
    Local,
    Order,
    Wish
}

pub trait EventBase: mopa::Any + Debug + Sync + Send {
    fn type_id(&self) -> any::TypeId;
    fn write(&self, writer: &mut Writer) -> Result<()>;
}

impl<T: Any + Debug + BitStore + Sync + Send> EventBase for T {
    fn type_id(&self) -> any::TypeId {
        any::TypeId::of::<T>()
    }

    fn write(&self, writer: &mut Writer) -> Result<()> {
        self.write_to(writer)
    }
}

pub trait Event: EventBase {
    fn class(&self) -> Class {
        Class::Order
    }
}

mopafy!(Event);

pub type EventBox = Box<Event>;

macro_rules! match_event {
    {
        $event:ident:
        $($typ:ty => $body:expr),*,
    } => {
        #[allow(unused)]
        {
            $(
                if let Some($event) = $event.downcast_ref::<$typ>() {
                    $body;
                }
            )*
        };
    };
}

/// Event type
#[derive(Clone)]
struct Type {
    pub read: fn(&mut Reader) -> Result<EventBox>,
}

fn read_event<T: Event + BitStore + Send>(reader: &mut Reader) -> Result<EventBox> {
    Ok(Box::new(T::read_from(reader)?))
}

#[derive(Clone)]
pub struct Registry {
    /// Event types, indexed by TypeIndex
    types: Vec<Type>,

    /// Map from TypeId to index into `types`
    type_indices: BTreeMap<any::TypeId, TypeIndex>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            types: Vec::new(),
            type_indices: BTreeMap::new(),
        }
    }

    pub fn register<T: Event + BitStore + Send>(&mut self) {
        assert!(
            self.types.len() <= u16::MAX as usize,
            "too many event types"
        );

        let type_id = any::TypeId::of::<T>();
        let type_index = self.types.len() as u16;

        let event_type = Type {
            read: read_event::<T>,
        };

        self.types.push(event_type);
        self.type_indices.insert(type_id, type_index);
    }

    pub fn write(&self, event: &Event, writer: &mut Writer) -> Result<()> {
        let type_id = event.type_id();
        let type_index = self.type_indices.get(&type_id).unwrap();

        writer.write(type_index)?;
        event.write(writer)
    }

    pub fn read(&self, reader: &mut Reader) -> Result<EventBox> {
        let type_index = reader.read::<TypeIndex>()?;

        let event_type = &self.types[type_index as usize];
        (event_type.read)(reader)
    }
}

pub struct Sink {
    events: Vec<EventBox>,
}

impl Sink {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    pub fn push<T: Event + Send>(&mut self, event: T) {
        self.push_box(Box::new(event));
    }

    pub fn push_box(&mut self, event: EventBox) {
        self.events.push(event);
    }

    pub fn into_inner(self) -> Vec<EventBox> {
        self.events
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use bit_manager::{BitRead, BitReader, BitWrite, BitWriter};

    use super::{Event, EventBox, Registry};

    #[derive(Debug, BitStore)]
    struct A;

    #[derive(Debug, BitStore)]
    struct B(bool);

    #[derive(Debug, BitStore, PartialEq, Eq)]
    enum C {
        X,
        Y(i32, bool),
    }

    impl Event for A {}
    impl Event for B {}
    impl Event for C {}

    #[test]
    fn test_match() {
        let event: EventBox = Box::new(A);

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
        let mut reg = Registry::new();
        reg.register::<A>();
        reg.register::<B>();
        reg.register::<C>();

        let event: EventBox = Box::new(C::Y(42, true));

        let data = {
            let mut writer = BitWriter::new(Vec::new());
            reg.write(&*event, &mut writer).unwrap();
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

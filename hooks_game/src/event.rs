use std::any::{self, Any};
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::io::Cursor;
use std::mem;
use std::u16;

use bit_manager::data::BitStore;
use bit_manager::{self, BitRead, BitReader, BitWrite, BitWriter};

use mopa;

#[derive(Debug)]
pub enum Error {
    InvalidTypeIndex(TypeIndex),
    BitManager(bit_manager::Error),
}

impl From<bit_manager::Error> for Error {
    fn from(error: bit_manager::Error) -> Error {
        Error::BitManager(error)
    }
}

pub type TypeIndex = u16;

pub type Writer = BitWriter<Vec<u8>>;
pub type Reader = BitReader<Cursor<Vec<u8>>>;

pub enum Class {
    Local,
    Order,
}

pub trait EventBase: mopa::Any + Debug + Sync + Send {
    fn type_id(&self) -> any::TypeId;
    fn write(&self, writer: &mut Writer) -> bit_manager::Result<()>;
}

impl<T: Any + Debug + Clone + BitStore + Sync + Send> EventBase for T {
    fn type_id(&self) -> any::TypeId {
        any::TypeId::of::<T>()
    }

    fn write(&self, writer: &mut Writer) -> bit_manager::Result<()> {
        self.write_to(writer)
    }
}

pub trait Event: EventBase + EventClone {
    fn class(&self) -> Class;
}

pub trait EventClone {
    fn clone_event(&self) -> Box<Event>;
}

impl<T: Clone + Event> EventClone for T {
    fn clone_event(&self) -> Box<Event> {
        Box::new(Clone::clone(self))
    }
}

mopafy!(Event);

#[macro_export]
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
    pub read: fn(&mut Reader) -> bit_manager::Result<Box<Event>>,
}

fn read_event<T: Event + BitStore>(reader: &mut Reader) -> bit_manager::Result<Box<Event>> {
    Ok(Box::new(T::read_from(reader)?))
}

#[derive(Clone, Default)]
pub struct Registry {
    /// Event types, indexed by TypeIndex
    types: Vec<Type>,

    /// Map from TypeId to index into `types`
    type_indices: BTreeMap<any::TypeId, TypeIndex>,
}

impl Registry {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn register<T: Event + BitStore>(&mut self) {
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

    pub fn write(&self, event: &Event, writer: &mut Writer) -> Result<(), bit_manager::Error> {
        let type_id = event.type_id();
        let type_index = self.type_indices[&type_id];

        writer.write(&type_index)?;
        event.write(writer)
    }

    pub fn read(&self, reader: &mut Reader) -> Result<Box<Event>, Error> {
        let type_index = reader.read::<TypeIndex>()?;

        if let Some(event_type) = self.types.get(type_index as usize) {
            Ok((event_type.read)(reader)?)
        } else {
            Err(Error::InvalidTypeIndex(type_index))
        }
    }
}

#[derive(Default)]
pub struct Sink {
    events: Vec<Box<Event>>,
    ignore: bool,
}

impl Sink {
    pub fn new() -> Sink {
        Sink {
            events: Vec::new(),
            ignore: false,
        }
    }

    pub fn clone_from_slice(events: &[Box<Event>]) -> Sink {
        let events = events.iter().map(|event| (**event).clone_event()).collect();
        Sink {
            events,
            ignore: false,
        }
    }

    pub fn push<T: Event + Send>(&mut self, event: T) {
        self.push_box(Box::new(event));
    }

    pub fn push_box(&mut self, event: Box<Event>) {
        if !self.ignore {
            self.events.push(event);
        }
    }

    pub fn clear(&mut self) -> Vec<Box<Event>> {
        mem::replace(&mut self.events, Vec::new())
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn iter(&self) -> &[Box<Event>] {
        &self.events
    }

    pub fn into_vec(self) -> Vec<Box<Event>> {
        self.events
    }

    pub fn set_ignore(&mut self, b: bool) -> bool {
        let old_b = self.ignore;
        self.ignore = b;
        old_b
    }
}

impl Clone for Sink {
    fn clone(self: &Sink) -> Sink {
        Sink::clone_from_slice(&self.events)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use bit_manager::{BitRead, BitReader, BitWrite, BitWriter};

    use super::{Class, Event, Registry};

    #[derive(Debug, BitStore)]
    struct A;

    #[derive(Debug, BitStore)]
    struct B(bool);

    #[derive(Debug, BitStore, PartialEq, Eq)]
    enum C {
        X,
        Y(i32, bool),
    }

    impl Event for A {
        fn class(&self) -> Class {
            Class::Local
        }
    }
    impl Event for B {
        fn class(&self) -> Class {
            Class::Local
        }
    }
    impl Event for C {
        fn class(&self) -> Class {
            Class::Local
        }
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
        let mut reg = Registry::new();
        reg.register::<A>();
        reg.register::<B>();
        reg.register::<C>();

        let event: Box<Event> = Box::new(C::Y(42, true));

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

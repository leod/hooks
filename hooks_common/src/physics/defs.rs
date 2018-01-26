use bit_manager::{BitRead, BitWrite, Result};
use bit_manager::data::BitStore;

use nalgebra::{Point2, Vector2};
use specs::{Component, FlaggedStorage, VecStorage};

use registry::Registry;

pub fn register(reg: &mut Registry) {
    reg.component::<Mass>();
    reg.component::<Position>();
    reg.component::<Velocity>();
    reg.component::<Orientation>();
}

/// Physical mass.
#[derive(PartialEq, Clone, Debug)]
pub struct Mass(pub f32);

impl Component for Mass {
    type Storage = VecStorage<Self>;
}

/// Two-dimensional position.
#[derive(PartialEq, Clone, Debug)]
pub struct Position(pub Point2<f32>);

impl Component for Position {
    type Storage = FlaggedStorage<Self, VecStorage<Self>>;
}

/// Two-dimensional velocity.
#[derive(PartialEq, Clone, Debug)]
pub struct Velocity(pub Vector2<f32>);

impl Component for Velocity {
    type Storage = VecStorage<Self>;
}

/// Rotation angle.
#[derive(PartialEq, Clone, Debug, BitStore)]
pub struct Orientation(pub f32);

impl Component for Orientation {
    type Storage = FlaggedStorage<Self, VecStorage<Self>>;
}

impl BitStore for Velocity {
    fn read_from<R: BitRead>(reader: &mut R) -> Result<Self> {
        Ok(Velocity(Vector2::new(reader.read()?, reader.read()?)))
    }

    fn write_to<W: BitWrite>(&self, writer: &mut W) -> Result<()> {
        writer.write(&self.0.x)?;
        writer.write(&self.0.y)
    }
}

impl BitStore for Position {
    fn read_from<R: BitRead>(reader: &mut R) -> Result<Self> {
        Ok(Position(Point2::new(reader.read()?, reader.read()?)))
    }

    fn write_to<W: BitWrite>(&self, writer: &mut W) -> Result<()> {
        writer.write(&self.0.x)?;
        writer.write(&self.0.y)
    }
}

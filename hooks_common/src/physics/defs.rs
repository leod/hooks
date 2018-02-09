use bit_manager::{BitRead, BitWrite, Result};
use bit_manager::data::BitStore;

use nalgebra::{Point2, Vector2};
use specs::{Component, FlaggedStorage, VecStorage};

use defs::EntityId;
use registry::Registry;

pub fn register(reg: &mut Registry) {
    reg.component::<Mass>();
    reg.component::<Dynamic>();

    reg.component::<Position>();
    reg.component::<Velocity>();
    reg.component::<Orientation>();
    reg.component::<Joints>();
}

/// Physical mass.
#[derive(Component, PartialEq, Clone, Debug)]
#[component(VecStorage)]
pub struct Mass(pub f32);

/// Two-dimensional velocity.
#[derive(Component, PartialEq, Clone, Debug)]
#[component(VecStorage)]
pub struct Velocity(pub Vector2<f32>);

/// Two-dimensional position.
#[derive(PartialEq, Clone, Debug)]
pub struct Position(pub Point2<f32>);

impl Component for Position {
    type Storage = FlaggedStorage<Self, VecStorage<Self>>;
}

/// Rotation angle.
#[derive(PartialEq, Clone, Debug, BitStore)]
pub struct Orientation(pub f32);

impl Component for Orientation {
    type Storage = FlaggedStorage<Self, VecStorage<Self>>;
}

/// Non-static entities
#[derive(Component, PartialEq, Clone, Debug)]
#[component(NullStorage)]
pub struct Dynamic;

/// Some kind of joint thingy.
#[derive(PartialEq, Clone, Debug, BitStore)]
pub struct Joint {
    strength: f32,
}

/// Entities that this entity is joined to.
#[derive(Component, PartialEq, Clone, Debug)]
#[component(BTreeStorage)]
pub struct Joints(pub Vec<(EntityId, Joint)>);

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

impl BitStore for Joints {
    fn read_from<R: BitRead>(reader: &mut R) -> Result<Self> {
        let mut joints = Vec::new();
        while reader.read_bit()? {
            joints.push(reader.read()?);
        }

        Ok(Joints(joints))
    }

    fn write_to<W: BitWrite>(&self, writer: &mut W) -> Result<()> {
        for joint in &self.0 {
            writer.write_bit(true)?;
            writer.write(joint)?;
        }

        writer.write_bit(false)
    }
}

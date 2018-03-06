use bit_manager::{BitRead, BitWrite, Result};
use bit_manager::data::BitStore;

use nalgebra::{Point2, Vector2};
use specs::{self, Component, FlaggedStorage, VecStorage};

use registry::Registry;
use repl::interp::Interp;

pub fn register(reg: &mut Registry) {
    reg.component::<Update>();
    reg.component::<Dynamic>();
    reg.component::<InvMass>();
    reg.component::<InvAngularMass>();
    reg.component::<Position>();
    reg.component::<Velocity>();
    reg.component::<Orientation>();
    reg.component::<AngularVelocity>();
    reg.component::<Friction>();
    reg.component::<Joints>();
}

/// Should this entity be updated in the next simulation run?
#[derive(Component, PartialEq, Clone, Debug)]
#[component(NullStorage)]
pub struct Update;

/// Non-static entities.
#[derive(Component, PartialEq, Clone, Debug)]
#[component(NullStorage)]
pub struct Dynamic;

/// Physical mass.
#[derive(Component, PartialEq, Clone, Debug)]
#[component(VecStorage)]
pub struct InvMass(pub f32);

/// Angular inertia.
#[derive(Component, PartialEq, Clone, Debug)]
#[component(VecStorage)]
pub struct InvAngularMass(pub f32);

/// Two-dimensional position.
#[derive(PartialEq, Clone, Debug)]
pub struct Position(pub Point2<f32>);

/// Two-dimensional velocity.
#[derive(Component, PartialEq, Clone, Debug)]
#[component(VecStorage)]
pub struct Velocity(pub Vector2<f32>);

impl Component for Position {
    type Storage = FlaggedStorage<Self, VecStorage<Self>>;
}

impl Interp for Position {
    fn interp(&self, other: &Position, t: f32) -> Position {
        //debug!("{} into {} at {}", self.0, other.0, t);
        Position(self.0 * (1.0 - t) + other.0.coords * t)
    }
}

/// Rotation angle.
#[derive(PartialEq, Clone, Debug, BitStore)]
pub struct Orientation(pub f32);

impl Interp for Orientation {
    fn interp(&self, other: &Orientation, t: f32) -> Orientation {
        // TODO: Solution for orientation interpolation without sin/cos/atan2?
        let x = (1.0 - t) * self.0.cos() + t * other.0.cos();
        let y = (1.0 - t) * self.0.sin() + t * other.0.sin();
        Orientation(y.atan2(x))
    }
}

impl Component for Orientation {
    type Storage = FlaggedStorage<Self, VecStorage<Self>>;
}

/// Angular velocity.
#[derive(Component, PartialEq, Clone, Debug, BitStore)]
#[component(VecStorage)]
pub struct AngularVelocity(pub f32);

/// Whether to apply friction to this entity.
#[derive(Component, PartialEq, Clone, Debug)]
#[component(NullStorage)]
pub struct Friction(pub f32);

/// Some kind of joint thingy.
#[derive(PartialEq, Clone, Debug, BitStore)]
pub struct Joint {
    pub stiffness: f32,
    pub resting_length: f32,
}

/// Entities that this entity is joined to.
#[derive(Component, PartialEq, Clone, Debug)]
#[component(BTreeStorage)]
pub struct Joints(pub Vec<(specs::Entity, Joint)>);

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

/*impl BitStore for Joints {
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
}*/

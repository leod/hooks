use bit_manager::data::BitStore;
use bit_manager::{BitRead, BitWrite, Result};

use nalgebra::{Point2, Rotation2, Vector2};

use specs::prelude::{Component, DenseVecStorage, FlaggedStorage, VecStorage};
use specs::storage::NullStorage;

use registry::Registry;
use repl;
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
    reg.component::<Drag>();
    //reg.component::<Joints>();
}

/// Should this entity be updated in the next simulation run?
#[derive(Component, PartialEq, Clone, Debug, Default)]
#[storage(NullStorage)]
pub struct Update;

/// Non-static entities.
#[derive(Component, PartialEq, Clone, Debug, Default)]
#[storage(NullStorage)]
pub struct Dynamic;

/// Physical mass.
#[derive(Component, PartialEq, Clone, Debug)]
#[storage(VecStorage)]
pub struct InvMass(pub f32);

/// Angular inertia.
#[derive(Component, PartialEq, Clone, Debug)]
#[storage(VecStorage)]
pub struct InvAngularMass(pub f32);

/// Two-dimensional position.
#[derive(PartialEq, Clone, Copy, Debug)]
pub struct Position(pub Point2<f32>);

/// Two-dimensional velocity.
#[derive(Component, PartialEq, Clone, Copy, Debug)]
#[storage(VecStorage)]
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
#[derive(PartialEq, Clone, Copy, Debug, BitStore)]
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

impl Orientation {
    pub fn rotate(&self, pos: Point2<f32>) -> Vector2<f32> {
        // TODO: We might want to cache some of these calculations in the future.
        //       Two possibilities:
        //       1. Calculate sin/cos whenever an orientation is set.
        //          Disadvantage: might calculate even if never used.
        //       2. Calculate sin/cos lazily, storing the result in an Option.
        //          Disadvantage: rotate() needs mutable access
        Rotation2::new(self.0).matrix() * pos.coords
    }
}

/// Angular velocity.
#[derive(Component, PartialEq, Clone, Copy, Debug, BitStore)]
#[storage(VecStorage)]
pub struct AngularVelocity(pub f32);

/// Whether to apply friction to this entity.
#[derive(Component, PartialEq, Clone, Copy, Debug, Default)]
#[storage(DenseVecStorage)]
pub struct Friction(pub f32);

/// Whether to apply drag to this entity.
#[derive(Component, PartialEq, Clone, Copy, Debug, Default)]
#[storage(DenseVecStorage)]
pub struct Drag(pub f32);

/// Transform from object-space to world-space.
pub fn to_world_pos(
    position: &Position,
    orientation: &Orientation,
    object_pos: Point2<f32>,
) -> Point2<f32> {
    position.0 + orientation.rotate(object_pos)
}

/*/// Some kind of joint thingy.
#[derive(PartialEq, Clone, Debug, BitStore)]
pub struct Joint {
    pub stiffness: f32,
    pub resting_length: f32,
}

/// Entities that this entity is joined to.
#[derive(Component, PartialEq, Clone, Debug)]
#[storage(BTreeStorage)]
pub struct Joints(pub Vec<(specs::Entity, Joint)>);*/

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

impl repl::Component for Position {
    fn distance(&self, other: &Position) -> f32 {
        let d = self.0 - other.0;
        d.x.abs().max(d.y.abs())
    }
}

impl repl::Component for Velocity {
    fn distance(&self, other: &Velocity) -> f32 {
        let d = self.0 - other.0;
        d.x.abs().max(d.y.abs())
    }
}

impl repl::Component for Orientation {
    fn distance(&self, other: &Orientation) -> f32 {
        (self.0 - other.0).abs()
    }
}

impl repl::Component for AngularVelocity {
    fn distance(&self, other: &AngularVelocity) -> f32 {
        (self.0 - other.0).abs()
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

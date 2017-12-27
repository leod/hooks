use bit_manager::{BitRead, BitWrite, Result};
use bit_manager::data::BitStore;

use specs::VecStorage;
use nalgebra::Point2;
use ncollide::shape::ShapeHandle2;

#[derive(Component, PartialEq, Clone, Debug)]
#[component(VecStorage)]
pub struct Position {
    pub pos: Point2<f32>,
}

#[derive(Component, PartialEq, Clone, Debug, BitStore)]
#[component(VecStorage)]
pub struct Orientation {
    pub angle: f32,
}

#[derive(Clone, Component)]
#[component(VecStorage)]
pub struct CollisionShape {
    pub shape: ShapeHandle2<f32>,
}

impl BitStore for Position {
    fn read_from<R: BitRead>(reader: &mut R) -> Result<Self> {
        Ok(Position {
            pos: Point2::new(reader.read()?, reader.read()?),
        })
    }

    fn write_to<W: BitWrite>(&self, writer: &mut W) -> Result<()> {
        writer.write(&self.pos.x)?;
        writer.write(&self.pos.y)
    }
}

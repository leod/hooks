// Two great resources for velocity constraints:
//    http://myselph.de/gamePhysics/equalityConstraints.html
//    http://myselph.de/gamePhysics/inequalityConstraints.html
// (though the following are position-based constraints)

use std::f32;
use std::ops::Deref;

use specs::prelude::*;
use specs::storage::MaskedStorage;

use nalgebra::{dot, norm, Matrix2x6, Point2, Rotation2, RowVector6, Vector2};

use physics::{Orientation, Position};
use repl;

/// A `Pose` described the physical state of one entity relevant to constraint solving.
#[derive(Clone, Debug)]
pub struct Pose {
    pub pos: Point2<f32>,
    pub angle: f32,
}

impl Pose {
    /// Convenience method for constructing a `Pose` from a replicated entity that may not have all
    /// of the required components.
    pub fn from_entity<D1, D2>(
        positions: &Storage<Position, D1>,
        orientation: &Storage<Orientation, D2>,
        entity: Entity,
    ) -> Result<Pose, repl::Error>
    where
        D1: Deref<Target = MaskedStorage<Position>>,
        D2: Deref<Target = MaskedStorage<Orientation>>,
    {
        Ok(Pose {
            pos: repl::try(positions, entity)?.0,
            angle: repl::try(orientation, entity)?.0,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Mass {
    pub inv: f32,
    pub inv_angular: f32,
}

#[derive(Clone, Debug)]
pub enum Def {
    Joint {
        distance: f32,

        /// Object-space coordinates.
        object_pos_a: Point2<f32>,

        /// Object-space coordinates.
        object_pos_b: Point2<f32>,
    },
    Contact {
        normal: Vector2<f32>,
        margin: f32,

        /// Object-space coordinates.
        object_pos_a: Point2<f32>,

        /// Object-space coordinates.
        object_pos_b: Point2<f32>,
    },
    Angle {
        angle: f32,
    },
    Sum(Box<Def>, Box<Def>),
}

/// Which values can change in solving a constraint?
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Vars {
    pub pos: bool,
    pub angle: bool,
}

/// A constraint between two entities.
#[derive(Clone, Debug)]
pub struct Constraint {
    pub def: Def,
    pub stiffness: f32,
    pub entity_a: Entity,
    pub entity_b: Entity,
    pub vars_a: Vars,
    pub vars_b: Vars,
}

impl Mass {
    /// Set inverse mass to zero for elements that should not change.
    pub fn zero_out_constants(&self, vars: &Vars) -> Mass {
        Mass {
            inv: if vars.pos { self.inv } else { 0.0 },
            inv_angular: if vars.angle {
                self.inv_angular
            } else {
                0.0
            },
        }
    }
}

impl Def {
    /// Calculate the constraint value as well as the jacobian at some position.
    #[inline]
    pub fn calculate(&self, x_a: &Pose, x_b: &Pose) -> (f32, RowVector6<f32>) {
        match *self {
            Def::Joint {
                distance,
                object_pos_a,
                object_pos_b,
            } => {
                let rot_a = *Rotation2::new(x_a.angle).matrix();
                let rot_b = *Rotation2::new(x_b.angle).matrix();
                let pos_a = rot_a * object_pos_a.coords + x_a.pos.coords;
                let pos_b = rot_b * object_pos_b.coords + x_b.pos.coords;

                let f = pos_a - pos_b;
                let value_f = norm(&f);
                let value = value_f - distance;

                if value_f < 1e-12 {
                    // TODO: No idea
                    //debug!("small value_f");
                    return (
                        value,
                        RowVector6::<f32>::new(0.001, 0.0, 0.0, 0.0, 0.0, 0.0),
                    );
                }

                // Smart boi
                let f_norm = f / value_f;
                let obj_a_rot = rot_a * object_pos_a.coords;
                let obj_b_rot = rot_b * object_pos_b.coords;
                let obj_a_rot_norm = norm(&obj_a_rot);
                let obj_b_rot_norm = norm(&obj_b_rot);
                let rot_impact_a = if obj_a_rot_norm > 1e-9 {
                    f_norm.perp(&(obj_a_rot / obj_a_rot_norm)).abs()
                } else {
                    0.0
                };
                let rot_impact_b = if obj_b_rot_norm > 1e-9 {
                    f_norm.perp(&(obj_b_rot / obj_b_rot_norm)).abs()
                } else {
                    0.0
                };

                let deriv_rot_a =
                    *Rotation2::new(x_a.angle + f32::consts::PI / 2.0).matrix() * rot_impact_a;
                let deriv_rot_b =
                    *Rotation2::new(x_b.angle + f32::consts::PI / 2.0).matrix() * rot_impact_b;

                let jacobian_f = Matrix2x6::new(
                    1.0,
                    0.0,
                    object_pos_a.coords.x * deriv_rot_a.m11 +
                        object_pos_a.coords.y * deriv_rot_a.m12,
                    -1.0,
                    0.0,
                    -object_pos_b.coords.x * deriv_rot_b.m11 -
                        object_pos_b.coords.y * deriv_rot_b.m12,
                    0.0,
                    1.0,
                    object_pos_a.coords.x * deriv_rot_a.m21 +
                        object_pos_a.coords.y * deriv_rot_a.m22,
                    0.0,
                    -1.0,
                    -object_pos_b.coords.x * deriv_rot_b.m21 -
                        object_pos_b.coords.y * deriv_rot_b.m22,
                );
                let jacobian = (jacobian_f.transpose() * f / value_f).transpose();

                /*let value = value_f * value_f - distance * distance;
				let jacobian = 2.0 * RowVector6::new(
					pos_a.x - pos_b.x,
					pos_a.y - pos_b.y,
					(pos_b - pos_a).perp(&(pos_a - x_a.p.coords)),
					pos_b.x - pos_a.x,
					pos_b.y - pos_a.y,
					(pos_a - pos_b).perp(&(pos_b - x_b.p.coords)),
				);*/

                (value, jacobian)
            }
            Def::Angle { angle } => {
                let value = x_a.angle - x_b.angle - angle;
                let jacobian = RowVector6::new(0.0, 0.0, 1.0, 0.0, 0.0, -1.0);
                (value, jacobian)
            }
            Def::Contact {
                normal,
                margin,
                object_pos_a,
                object_pos_b,
            } => {
                // TODO: Create functions for this stuff
                let rot_a = *Rotation2::new(x_a.angle).matrix();
                let rot_b = *Rotation2::new(x_b.angle).matrix();
                let deriv_rot_a = *Rotation2::new(x_a.angle + f32::consts::PI / 2.0).matrix();
                let deriv_rot_b = *Rotation2::new(x_b.angle + f32::consts::PI / 2.0).matrix();
                let pos_a = rot_a * object_pos_a.coords + x_a.pos.coords;
                let pos_b = rot_b * object_pos_b.coords + x_b.pos.coords;

                let value = dot(&(pos_a - pos_b), &normal) - margin;
                let jacobian = RowVector6::new(
                    normal.x,
                    normal.y,
                    dot(&(deriv_rot_a * object_pos_a.coords), &normal),
                    -normal.x,
                    -normal.y,
                    -dot(&(deriv_rot_b * object_pos_b.coords), &normal),
                );

                (-value, -jacobian)
            }
            Def::Sum(ref k1, ref k2) => {
                let (value_1, jacobian_1) = k1.calculate(x_a, x_b);
                let (value_2, jacobian_2) = k2.calculate(x_a, x_b);

                (value_1 + value_2, jacobian_1 + jacobian_2)
            }
        }
    }

    /// Is this an inequality constraint, i.e. `C >= 0`, or an equality constraint, i.e. `C = 0`?
    pub fn is_inequality(&self) -> bool {
        match *self {
            Def::Joint { .. } => false,
            Def::Angle { .. } => false,
            Def::Contact { .. } => true,
            Def::Sum(_, _) => false,
        }
    }
}

/// Solve for the velocity update of one constraint.
pub fn solve_for_position(
    constraint: &Def,
    stiffness: f32,
    x_a: &Pose,
    x_b: &Pose,
    m_a: &Mass,
    m_b: &Mass,
) -> (Pose, Pose) {
    let inv_m = RowVector6::new(
        m_a.inv,
        m_a.inv,
        m_a.inv_angular,
        m_b.inv,
        m_b.inv,
        m_b.inv_angular,
    );
    let (value, jacobian) = constraint.calculate(x_a, x_b);

    if (constraint.is_inequality() && value >= 0.0) || value.abs() <= 1e-9 {
        return (x_a.clone(), x_b.clone());
    }

    let denom = dot(&jacobian.component_mul(&inv_m), &jacobian);

    if denom <= 1e-9 {
        // TODO: No idea
        debug!("small denom");
        return (x_a.clone(), x_b.clone());
    }

    let lambda = value / denom;

    let delta = -lambda * stiffness * jacobian.component_mul(&inv_m).transpose();

    (
        Pose {
            pos: x_a.pos + Vector2::new(delta.x, delta.y),
            angle: x_a.angle + delta.z,
        },
        Pose {
            pos: x_b.pos + Vector2::new(delta.w, delta.a),
            angle: x_b.angle + delta.b,
        },
    )
}

// Two great resources for velocity constraints:
//    http://myselph.de/gamePhysics/equalityConstraints.html
//    http://myselph.de/gamePhysics/inequalityConstraints.html

use std::f32;

use specs::Entity;

use nalgebra::{dot, norm, Matrix2x6, Point2, Rotation2, RowVector6, Vector2};

#[derive(Clone, Debug)]
pub struct Position {
    pub p: Point2<f32>,
    pub angle: f32,
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
        p_object_a: Point2<f32>,

        /// Object-space coordinates.
        p_object_b: Point2<f32>,
    },
    Contact {
        normal: Vector2<f32>,
        margin: f32,

        /// Object-space coordinates.
        p_object_a: Point2<f32>,

        /// Object-space coordinates.
        p_object_b: Point2<f32>,
    },
    Angle {
        angle: f32,
    },
    Sum(Box<Def>, Box<Def>),
}

/// Which values can change in solving a constraint?
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Vars {
    // TODO: These two are kind of misnomers, they should refer to velocity
    pub p: bool,
    pub angle: bool,
}

/// A constraint between two entities.
#[derive(Clone, Debug)]
pub struct Constraint {
    pub entity_a: Entity,
    pub entity_b: Entity,
    pub vars_a: Vars,
    pub vars_b: Vars,
    pub def: Def,
}

impl Mass {
    /// Set inverse mass to zero for elements that should not change.
    pub fn zero_out_constants(&self, vars: &Vars) -> Mass {
        Mass {
            inv: if vars.p { self.inv } else { 0.0 },
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
    pub fn calculate(&self, x_a: &Position, x_b: &Position) -> (f32, RowVector6<f32>) {
        match self {
            &Def::Joint {
                distance,
                p_object_a,
                p_object_b,
            } => {
                let rot_a = Rotation2::new(x_a.angle).matrix().clone();
                let rot_b = Rotation2::new(x_b.angle).matrix().clone();
                let deriv_rot_a = Rotation2::new(x_a.angle + f32::consts::PI / 2.0)
                    .matrix()
                    .clone();
                let deriv_rot_b = Rotation2::new(x_b.angle + f32::consts::PI / 2.0)
                    .matrix()
                    .clone();
                let p_a = rot_a * p_object_a.coords + x_a.p.coords;
                let p_b = rot_b * p_object_b.coords + x_b.p.coords;

                let f = p_a - p_b;
                let value_f = norm(&f);
                let value = value_f - distance;
                let jacobian_f = Matrix2x6::new(
                    1.0,
                    0.0,
                    p_object_a.coords.x * deriv_rot_a.m11 + p_object_a.coords.y * deriv_rot_a.m12,
                    -1.0,
                    0.0,
                    -p_object_b.coords.x * deriv_rot_b.m11 - p_object_b.coords.y * deriv_rot_b.m12,
                    0.0,
                    1.0,
                    p_object_a.coords.x * deriv_rot_a.m21 + p_object_a.coords.y * deriv_rot_a.m22,
                    0.0,
                    -1.0,
                    -p_object_b.coords.x * deriv_rot_b.m21 - p_object_b.coords.y * deriv_rot_b.m22,
                );
                let jacobian = jacobian_f.transpose() * f / value_f;

                //debug!("f {}", f);
                //debug!("jacobian_f {}", jacobian_f);

                (value, jacobian.transpose())
            }
            &Def::Angle { angle } => {
                let value = x_a.angle - x_b.angle - angle;
                let jacobian = RowVector6::new(0.0, 0.0, 1.0, 0.0, 0.0, -1.0);
                (value, jacobian)
            }
            &Def::Contact {
                normal,
                margin,
                p_object_a,
                p_object_b,
            } => {
                // TODO: Create functions for this stuff
                let rot_a = Rotation2::new(x_a.angle).matrix().clone();
                let rot_b = Rotation2::new(x_b.angle).matrix().clone();
                let deriv_rot_a = Rotation2::new(x_a.angle + f32::consts::PI / 2.0)
                    .matrix()
                    .clone();
                let deriv_rot_b = Rotation2::new(x_b.angle + f32::consts::PI / 2.0)
                    .matrix()
                    .clone();
                let p_a = rot_a * p_object_a.coords + x_a.p.coords;
                let p_b = rot_b * p_object_b.coords + x_b.p.coords;

                let value = dot(&(p_a - p_b), &normal) - margin;
                let jacobian = RowVector6::new(
                    normal.x,
                    normal.y,
                    dot(&(deriv_rot_a * p_object_a.coords), &normal),
                    -normal.x,
                    -normal.y,
                    -dot(&(deriv_rot_b * p_object_b.coords), &normal),
                );

                (value, jacobian)
            }
            &Def::Sum(ref k1, ref k2) => {
                let (value_1, jacobian_1) = k1.calculate(x_a, x_b);
                let (value_2, jacobian_2) = k2.calculate(x_a, x_b);

                (value_1 + value_2, jacobian_1 + jacobian_2)
            }
        }
    }

    /// Is this an inequality constraint, i.e. `C >= 0`, or an equality constraint, i.e. `C = 0`?
    pub fn is_inequality(&self) -> bool {
        match self {
            &Def::Joint { .. } => false,
            &Def::Angle { .. } => false,
            &Def::Contact { .. } => true,
            &Def::Sum(_, _) => false,
        }
    }
}

/// Solve for the velocity update of one constraint.
pub fn solve_for_position(
    constraint: &Def,
    x_a: &Position,
    x_b: &Position,
    m_a: &Mass,
    m_b: &Mass,
) -> (Position, Position) {
    let inv_m = RowVector6::new(
        m_a.inv,
        m_a.inv,
        m_a.inv_angular,
        m_b.inv,
        m_b.inv,
        m_b.inv_angular,
    );
    let (value, jacobian) = constraint.calculate(x_a, x_b);

    if value.abs() <= 0.0001 {
        return (x_a.clone(), x_b.clone());
    }

    let lambda = value / dot(&jacobian.component_mul(&inv_m), &jacobian);
    //debug!("{} {}", value, lambda);

    let delta = -lambda * 1.0 * jacobian.component_mul(&inv_m).transpose();

    (
        Position {
            p: x_a.p + Vector2::new(delta.x, delta.y),
            angle: x_a.angle + delta.z,
        },
        Position {
            p: x_b.p + Vector2::new(delta.w, delta.a),
            angle: x_b.angle + delta.b,
        },
    )
}

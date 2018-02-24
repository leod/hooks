// Two great resources for velocity constraints:
//    http://myselph.de/gamePhysics/equalityConstraints.html
//    http://myselph.de/gamePhysics/inequalityConstraints.html

use std::f32;

use specs::Entity;

use nalgebra::{dot, norm, Matrix2, Matrix2x6, Point2, Rotation2, RowVector6, Vector2, Vector6};

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
pub enum Kind {
    Joint { distance: f32 },
    Contact { normal: Vector2<f32> },
}

#[derive(Clone, Debug)]
pub struct Def {
    pub kind: Kind,

    /// Object-space coordinates.
    pub p_object_a: Point2<f32>,

    /// Object-space coordinates.
    pub p_object_b: Point2<f32>,
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
        let rot_a = Rotation2::new(x_a.angle).matrix().clone();
        let rot_b = Rotation2::new(x_b.angle).matrix().clone();
        let deriv_rot_a = Rotation2::new(x_a.angle + f32::consts::PI / 2.0)
            .matrix()
            .clone();
        let deriv_rot_b = Rotation2::new(x_b.angle + f32::consts::PI / 2.0)
            .matrix()
            .clone();

        let p_a = rot_a * self.p_object_a.coords + x_a.p.coords;
        let p_b = rot_b * self.p_object_b.coords + x_b.p.coords;

        match self.kind {
            Kind::Joint { distance } => {
                let f = p_a - p_b;
                let value_f = norm(&f);
                let value = value_f - distance;
                let jacobian_f = Matrix2x6::new(
                    1.0,
                    0.0,
                    self.p_object_a.coords.x * deriv_rot_a.m11 +
                        self.p_object_a.coords.y * deriv_rot_a.m12,
                    -1.0,
                    0.0,
                    -self.p_object_b.coords.x * deriv_rot_b.m11 -
                        self.p_object_b.coords.y * deriv_rot_b.m12,
                    0.0,
                    1.0,
                    self.p_object_a.coords.x * deriv_rot_a.m21 +
                        self.p_object_a.coords.y * deriv_rot_a.m22,
                    0.0,
                    -1.0,
                    -self.p_object_b.coords.x * deriv_rot_b.m21 -
                        self.p_object_b.coords.y * deriv_rot_b.m22,
                );
                let jacobian = jacobian_f.transpose() * f / value_f;

                //debug!("f {}", f);
                //debug!("jacobian_f {}", jacobian_f);

                (value, jacobian.transpose())
            }
            Kind::Contact { normal } => {
                unimplemented!();
            }
        }
    }

    /// Is this an inequality constraint, i.e. `C >= 0`, or an equality constraint, i.e. `C = 0`?
    pub fn is_inequality(&self) -> bool {
        match self.kind {
            Kind::Joint { .. } => false,
            Kind::Contact { .. } => true,
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

    if value <= 0.0001 {
        return (x_a.clone(), x_b.clone());
    }

    let lambda = value / dot(&jacobian.component_mul(&inv_m), &jacobian);

    let delta = -lambda * jacobian.component_mul(&inv_m).transpose();

    //debug!("value {}", value);
    //debug!("jacobian {}", jacobian);
    //debug!("error {}", value + dot(&jacobian, &delta.transpose()));
    //debug!("delta {}", delta);
    //panic!("bubadu");

    /*(Position {
        p: x_a.p + Vector2::new(delta.w, delta.a), 
        angle: x_a.angle + delta.b,
    },
    Position {
        p: x_b.p + Vector2::new(delta.x, delta.y), 
        angle: x_b.angle + delta.z,
    })*/

    /*(
        Position {
            p: x_a.p + Vector2::new(delta.x, delta.y),
            angle: x_a.angle + delta.z.atan(),
        },
        Position {
            p: x_b.p + Vector2::new(delta.w, delta.a),
            angle: x_b.angle + delta.b.atan(),
        },
    )*/

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

    /*(Position {
        p: x_a.p,
        angle: x_a.angle,
    },
    Position {
        p: x_b.p,
        angle: x_b.angle,
    })*/
}

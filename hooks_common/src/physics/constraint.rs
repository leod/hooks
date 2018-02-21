// http://myselph.de/gamePhysics/equalityConstraints.html

use specs::Entity;

use nalgebra::{dot, norm_squared, Point2, Rotation2, RowVector6, Vector2, Vector6};

#[derive(Clone, Debug)]
pub struct Position {
    pub p: Point2<f32>,
    pub angle: f32,
}

#[derive(Clone, Debug)]
pub struct Velocity {
    pub linear: Vector2<f32>,
    pub angular: f32,
}

#[derive(Clone, Debug)]
pub struct Mass {
    pub inv: f32,
    pub inv_angular: f32,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Kind {
    Joint,
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
        match self.kind {
            Kind::Joint => {
                let p_a =
                    Rotation2::new(x_a.angle).matrix() * self.p_object_a.coords + x_a.p.coords;
                let p_b =
                    Rotation2::new(x_b.angle).matrix() * self.p_object_b.coords + x_b.p.coords;

                let c = norm_squared(&(p_a - p_b));
                let jacobian = 2.0 *
                    RowVector6::new(
                        p_a.x - p_b.x,
                        p_a.y - p_b.y,
                        -(p_a - p_b).perp(&(p_a - x_a.p.coords)),
                        p_b.x - p_a.x,
                        p_b.y - p_a.y,
                        (p_a - p_b).perp(&(p_b - x_b.p.coords)),
                    );
                (c, jacobian)
            }
        }
    }
}

/// Solve for the velocity update of one constraint.
pub fn solve_for_velocity(
    constraint: &Def,
    x_a: &Position,
    x_b: &Position,
    v_a: &Velocity,
    v_b: &Velocity,
    m_a: &Mass,
    m_b: &Mass,
    beta: f32,
    dt: f32,
) -> (Velocity, Velocity) {
    let inv_m = RowVector6::new(
        m_a.inv,
        m_a.inv,
        m_a.inv_angular,
        m_b.inv,
        m_b.inv,
        m_b.inv_angular,
    );
    let v = Vector6::new(
        v_a.linear.x,
        v_a.linear.y,
        v_a.angular,
        v_b.linear.x,
        v_b.linear.y,
        v_b.angular,
    );

    let (value, jacobian) = constraint.calculate(x_a, x_b);
    let bias = beta / dt * value;

    let numerator = dot(&jacobian.transpose(), &v) + bias;
    let denumerator = dot(&jacobian.component_mul(&inv_m), &jacobian);
    let lambda = -numerator / denumerator;

    let v_new = v + lambda * jacobian.component_mul(&inv_m).transpose();

    (
        Velocity {
            linear: Vector2::new(v_new.x, v_new.y),
            angular: v_new.z,
        },
        Velocity {
            linear: Vector2::new(v_new.w, v_new.a),
            angular: v_new.b,
        },
    )
}
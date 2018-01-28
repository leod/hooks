use std::fmt;

// TODO: Consider lazy evaluation of inspection with a visitor trait.

pub enum Vars {
    Leaf(String),
    Node(Vec<(String, Vars)>),
}

pub trait Inspect {
    fn inspect(&self) -> Vars;
}

impl<T: fmt::Debug> Inspect for T {
    default fn inspect(&self) -> Vars {
        Vars::Leaf(format!("{:?}", self))
    }
}

impl Inspect for f32 {
    fn inspect(&self) -> Vars {
        Vars::Leaf(format!("{:.2}", self))
    }
}

impl Inspect for f64 {
    fn inspect(&self) -> Vars {
        Vars::Leaf(format!("{:.2}", self))
    }
}

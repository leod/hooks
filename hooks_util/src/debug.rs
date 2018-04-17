use std::fmt;
use std::io::Write;

// TODO: Consider lazy evaluation of inspection with a visitor trait.

pub enum Vars {
    Leaf(String),
    Node(Vec<(String, Vars)>),
}

pub trait Inspect {
    fn inspect(&self) -> Vars;
}

impl<T: fmt::Debug> Inspect for T {
    fn inspect(&self) -> Vars {
        Vars::Leaf(format!("{:?}", self))
    }
}

impl Vars {
    pub fn print<W: Write>(&self, write: &mut W) {
        self.print_recursive(write, 0);
    }

    fn print_recursive<W: Write>(&self, write: &mut W, depth: usize) {
        match self {
            &Vars::Leaf(ref string) => {
                write!(write, "{}\n", string).unwrap();
            }
            &Vars::Node(ref succs) => {
                let max_length = succs
                    .iter()
                    .map(|&(ref name, _)| name.chars().count())
                    .max()
                    .unwrap_or(0);

                for (i, &(ref name, ref vars)) in succs.iter().enumerate() {
                    if i > 0 {
                        for _ in 0..depth {
                            write!(write, " ").unwrap();
                        }
                    }
                    write!(write, "{} ", name).unwrap();
                    for _ in 0..max_length - name.chars().count() {
                        write!(write, " ").unwrap();
                    }
                    vars.print_recursive(write, depth + max_length + 1);
                }
            }
        }
    }
}

/*impl Inspect for f32 {
    fn inspect(&self) -> Vars {
        Vars::Leaf(format!("{:.2}", self))
    }
}

impl Inspect for f64 {
    fn inspect(&self) -> Vars {
        Vars::Leaf(format!("{:.2}", self))
    }
}*/

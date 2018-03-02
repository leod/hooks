// This module's implementation has been inspired by hprof:
// <https://cmr.github.io/hprof/src/hprof/lib.rs.html#306-308>

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use timer;
use debug;

thread_local!(pub static PROFILER: RefCell<Profiler> = RefCell::new(Profiler::new()));

#[macro_export]
macro_rules! profile {
    ($name:expr) => (let _guard = profile::PROFILER.with(|p| p.borrow_mut().enter($name));)
}

pub struct Node {
    pub name: &'static str,

    pred: Option<Rc<RefCell<Node>>>,
    succs: Vec<Rc<RefCell<Node>>>,

    num_calls: usize,
    start_instant: Option<Instant>,
    duration_sum: Duration,
}

impl Node {
    fn new(name: &'static str, pred: Option<Rc<RefCell<Node>>>) -> Node {
        Node {
            name,
            pred,
            succs: Vec::new(),
            num_calls: 0,
            start_instant: None,
            duration_sum: Duration::new(0, 0),
        }
    }

    fn enter(&mut self) -> Guard {
        self.num_calls += 1;
        self.start_instant = Some(Instant::now());
        Guard
    }

    fn leave(&mut self) {
        self.duration_sum = self.duration_sum
            .checked_add(self.start_instant.unwrap().elapsed())
            .unwrap();
    }
}

pub struct Guard;

impl Drop for Guard {
    fn drop(&mut self) {
        PROFILER.with(|p| p.borrow_mut().leave());
    }
}

pub struct Profiler {
    root: Rc<RefCell<Node>>,
    current: Rc<RefCell<Node>>,
}

impl Profiler {
    pub fn new() -> Profiler {
        let root = Rc::new(RefCell::new(Node::new("root", None)));

        Profiler {
            root: root.clone(),
            current: root.clone(),
        }
    }

    pub fn frame(&mut self) -> Guard {
        assert!(
            self.current.borrow().pred.is_none(),
            "should start frame at root profiling node"
        );

        let mut current = self.current.borrow_mut();
        current.enter()
    }

    pub fn enter(&mut self, name: &'static str) -> Guard {
        let existing_succ = {
            let current = self.current.borrow();

            current
                .succs
                .iter()
                .find(|succ| succ.borrow().name == name)
                .map(|succ| succ.clone())
        };

        let succ = if let Some(existing_succ) = existing_succ {
            existing_succ
        } else {
            let succ = Rc::new(RefCell::new(Node::new(name, Some(self.current.clone()))));
            self.current.borrow_mut().succs.push(succ.clone());
            succ
        };

        self.current = succ;
        self.current.borrow_mut().enter()
    }

    fn leave(&mut self) {
        self.current.borrow_mut().leave();

        if self.current.borrow().pred.is_some() {
            self.current = {
                let pred = self.current.borrow().pred.clone().unwrap();
                pred
            }
        }
    }
}

impl debug::Inspect for Node {
    fn inspect(&self) -> debug::Vars {
        let duration_sum_secs = timer::duration_to_secs(self.duration_sum);
        let percent = self.pred
            .clone()
            .map(|pred| duration_sum_secs / timer::duration_to_secs(pred.borrow().duration_sum))
            .unwrap_or(1.0) * 100.0;
        let root_duration_sum_secs = PROFILER.with(|p| {
            let p = p.borrow();
            let root = p.root.borrow();
            timer::duration_to_secs(root.duration_sum)
        });
        //let name = if self.pred.is_some() { "".to_string() } else { self.name.to_string() };
        let mut vars = vec![
            (
                ":".to_string(),
                debug::Vars::Leaf(format!(
                    "{:3.2}% {:>4.2}ms/call {:.2}Hz",
                    percent,
                    duration_sum_secs * 1000.0 / (self.num_calls as f64),
                    self.num_calls as f64 / root_duration_sum_secs
                )),
            ),
        ];

        if !self.succs.is_empty() {
            let mut succs = Vec::new();
            for succ in &self.succs {
                let succ = succ.borrow();
                succs.push((succ.name.to_string(), succ.inspect()));
            }

            vars.push(("↳".to_string(), debug::Vars::Node(succs)));
        }

        debug::Vars::Node(vars)
    }
}

impl debug::Inspect for Profiler {
    fn inspect(&self) -> debug::Vars {
        self.root.borrow().inspect()
    }
}

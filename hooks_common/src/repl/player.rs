use specs::World;

use defs::{PlayerId, PlayerInfo};
use registry::Registry;
use event::{self, Event};

pub fn register(reg: &mut Registry) {
    reg.event::<JoinedEvent>();
    reg.event::<LeftEvent>();
    reg.post_tick_event_handler(handle_post_tick_event);
}

#[derive(Debug, BitStore)]
pub struct JoinedEvent {
    pub id: PlayerId,
    pub info: PlayerInfo
}

impl Event for JoinedEvent {
    fn class(&self) -> event::Class {
        event::Class::Order
    }
}

#[derive(Debug, BitStore)]
pub struct LeftEvent {
    pub id: PlayerId,
}

impl Event for LeftEvent {
    fn class(&self) -> event::Class {
        event::Class::Order
    }
}

fn handle_post_tick_event(world: &World, event: &Box<Event>) {
    match_event!(event:
        JoinedEvent => {
            
        },
        LeftEvent => {
        },
    );
}

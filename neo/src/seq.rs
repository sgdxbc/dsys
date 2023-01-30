use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use dsys::Protocol;

use crate::{Effect, Event, Message};

#[derive(Default)]
pub struct Sequencer {
    seq: u32,
}

impl Protocol<Event> for Sequencer {
    type Effect = Effect;

    fn init(&mut self) -> Self::Effect {
        Effect::Nop
    }

    fn update(&mut self, event: Event) -> Self::Effect {
        let Event::Handle(Message::Request(mut multicast)) = event else {
            return Effect::Nop
        };
        self.seq += 1;
        multicast.seq = self.seq;
        let mut hasher = DefaultHasher::new();
        multicast.digest.hash(&mut hasher);
        multicast
            .signature
            .copy_from_slice(&hasher.finish().to_le_bytes()[..4]);
        Effect::Broadcast(Message::Request(multicast))
    }
}

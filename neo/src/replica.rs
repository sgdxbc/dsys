use std::{
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
    ops::{Index, IndexMut},
};

use bincode::Options;
use dsys::{App, Protocol};
use sha2::Digest;

use crate::{Effect, Event, Message, Multicast, Reply, Request};

pub struct Replica {
    id: u8,
    pub log: Vec<LogEntry>,
    reorder_request: HashMap<u32, Multicast>,
    app: App,
    replies: HashMap<u32, Reply>,
}

#[allow(unused)]
pub struct LogEntry {
    request: Request,
    seq: u32,
    signature: [u8; 4],
}

impl Replica {
    pub fn new(id: u8, app: App) -> Self {
        Self {
            id,
            log: Default::default(),
            reorder_request: Default::default(),
            app,
            replies: Default::default(),
        }
    }
}

impl Index<u32> for Replica {
    type Output = LogEntry;

    fn index(&self, index: u32) -> &Self::Output {
        &self.log[(index - 1) as usize]
    }
}

impl IndexMut<u32> for Replica {
    fn index_mut(&mut self, index: u32) -> &mut Self::Output {
        &mut self.log[(index - 1) as usize]
    }
}

impl Protocol<Event> for Replica {
    type Effect = Effect;

    fn init(&mut self) -> Self::Effect {
        Effect::Nop
    }

    fn update(&mut self, event: Event) -> Self::Effect {
        let Event::Handle(message) = event else {
            return Effect::Nop;
        };
        match message {
            Message::Request(multicast) => self.reorder_request(multicast),
            _ => Effect::Nop,
        }
    }
}

impl Replica {
    fn next_entry(&self) -> u32 {
        self.log.len() as u32 + 1
    }

    fn reorder_request(&mut self, multicast: Multicast) -> Effect {
        let verified = {
            let digest = <[u8; 32]>::from(sha2::Sha256::digest(&multicast.payload));
            let mut hasher = DefaultHasher::new();
            digest.hash(&mut hasher);
            &hasher.finish().to_le_bytes()[..4] == &multicast.signature
        };
        if !verified {
            println!("malformed");
            return Effect::Nop;
        }

        if multicast.seq != self.next_entry() {
            self.reorder_request.insert(multicast.seq, multicast);
            return Effect::Nop;
        }

        let mut effect = self.handle_request(multicast);
        while let Some(multicast) = self.reorder_request.remove(&self.next_entry()) {
            effect = effect + self.handle_request(multicast);
        }
        effect
    }

    fn handle_request(&mut self, multicast: Multicast) -> Effect {
        assert_eq!(multicast.seq, self.next_entry());
        let request = bincode::options()
            .deserialize::<Request>(&multicast.payload)
            .unwrap();
        self.log.push(LogEntry {
            request: request.clone(),
            seq: multicast.seq,
            signature: multicast.signature,
        });

        match self.replies.get(&request.client_id) {
            Some(reply) if reply.request_num > request.request_num => return Effect::Nop,
            Some(reply) if reply.request_num == request.request_num => {
                return Effect::Send(request.client_addr, Message::Reply(reply.clone()))
            }
            _ => {}
        }

        let result = self.app.execute(&request.op);
        let reply = Reply {
            request_num: request.request_num,
            replica_id: self.id,
            result,
            seq: multicast.seq,
        };
        self.replies.insert(request.client_id, reply.clone());
        Effect::Send(request.client_addr, Message::Reply(reply))
    }
}

use std::{
    collections::HashMap,
    ops::{Index, IndexMut},
};

use dsys::{protocol::Composite, App, Protocol};

use crate::{Message, Multicast, Reply, Request};

pub struct Replica {
    id: u8,
    pub log: Vec<LogEntry>,
    reorder_request: HashMap<u32, (Multicast, Request)>,
    app: App,
    replies: HashMap<u32, Reply>,
}

#[allow(unused)]
pub struct LogEntry {
    request: Request,
    seq: u32,
    // signature: [u32; 16],
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

type Event = dsys::NodeEvent<Message>;
type Effect = dsys::NodeEffect<Message>;

impl Protocol<Event> for Replica {
    type Effect = Effect;

    fn update(&mut self, event: Event) -> Self::Effect {
        let Event::Handle(message) = event else {
            return Effect::Nop;
        };
        match message {
            Message::OrderedRequest(multicast, request) => self.reorder_request(multicast, request),
            _ => Effect::Nop,
        }
    }
}

impl Replica {
    fn next_entry(&self) -> u32 {
        self.log.len() as u32 + 1
    }

    fn reorder_request(&mut self, multicast: Multicast, request: Request) -> Effect {
        if multicast.seq != self.next_entry() {
            self.reorder_request
                .insert(multicast.seq, (multicast, request));
            return Effect::Nop;
        }

        let mut effect = self.handle_request(multicast, request);
        while let Some((multicast, request)) = self.reorder_request.remove(&self.next_entry()) {
            effect = effect.compose(self.handle_request(multicast, request));
        }
        effect
    }

    fn handle_request(&mut self, multicast: Multicast, request: Request) -> Effect {
        assert_eq!(multicast.seq, self.next_entry());
        self.log.push(LogEntry {
            request: request.clone(),
            seq: multicast.seq,
            // signature: multicast.signature,
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

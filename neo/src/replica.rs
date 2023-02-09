use std::{
    collections::HashMap,
    ops::{Index, IndexMut},
};

use dsys::{protocol::Composite, App, Protocol};

use crate::{Message, Multicast, Reply, Request};

pub struct Replica {
    id: u8,
    f: usize,
    multicast_crypto: MulticastCrypto,
    pub log: Vec<LogEntry>,
    multicast_signatures: HashMap<u32, MulticastSignature>,
    reorder_request: HashMap<u32, Vec<(Multicast, Request)>>,
    app: App,
    replies: HashMap<u32, Reply>,
}

pub enum MulticastCrypto {
    SipHash,
    P256,
}

enum MulticastSignature {
    SipHash(HashMap<u8, [u8; 4]>),
    P256([[u8; 32]; 2]),
}

#[allow(unused)]
pub struct LogEntry {
    request: Request,
    //
}

impl Replica {
    pub fn new(id: u8, app: App, f: usize, multicast_crypto: MulticastCrypto) -> Self {
        Self {
            id,
            f,
            multicast_crypto,
            log: Default::default(),
            multicast_signatures: Default::default(),
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

    fn multicast_complete(&self, seq: u32) -> bool {
        match self.multicast_signatures.get(&seq) {
            None => false,
            Some(MulticastSignature::P256(_)) => true,
            Some(MulticastSignature::SipHash(signatures)) => signatures.len() == 3 * self.f + 1,
        }
    }

    fn ordered_entry(&self) -> u32 {
        let next = self.next_entry();
        if next == 1 || self.multicast_complete(next - 1) {
            next
        } else {
            next - 1
        }
    }

    fn reorder_request(&mut self, multicast: Multicast, request: Request) -> Effect {
        if multicast.seq != self.ordered_entry() {
            self.reorder_request
                .entry(multicast.seq)
                .or_default()
                .push((multicast, request));
            return Effect::Nop;
        }

        let mut effect = self.handle_request(multicast, request);
        while let Some(messages) = self.reorder_request.remove(&self.ordered_entry()) {
            for (multicast, request) in messages {
                effect = effect.compose(self.handle_request(multicast, request));
            }
        }
        effect
    }

    fn handle_request(&mut self, multicast: Multicast, request: Request) -> Effect {
        assert_eq!(multicast.seq, self.ordered_entry());
        if multicast.seq == self.next_entry() {
            self.log.push(LogEntry {
                request: request.clone(),
            });
        }
        if request != self[multicast.seq].request {
            //
            return Effect::Nop;
        }
        match self.multicast_crypto {
            MulticastCrypto::P256 => {
                self.multicast_signatures
                    .insert(multicast.seq, MulticastSignature::P256(multicast.signature));
            }
            MulticastCrypto::SipHash => {
                let MulticastSignature::SipHash(signatures) = self.multicast_signatures
                    .entry(multicast.seq)
                    .or_insert(MulticastSignature::SipHash(Default::default()))
                else {
                    unreachable!()
                };
                let i = u32::from_be_bytes(multicast.signature[0][..4].try_into().unwrap()) as u8;
                for j in i..u8::min(i + 4, (3 * self.f + 1) as _) {
                    let offset = 4 + (j - i) as usize * 4;
                    signatures.insert(
                        j,
                        multicast.signature[0][offset..offset + 4]
                            .try_into()
                            .unwrap(),
                    );
                }
            }
        }
        if !self.multicast_complete(multicast.seq) {
            return Effect::Nop;
        }

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

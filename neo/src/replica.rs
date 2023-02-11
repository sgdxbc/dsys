use std::{collections::HashMap, ops::Index};

use dsys::{protocol::Composite, App, NodeEffect, Protocol};
use sha2::Digest;

use crate::{Message, Multicast, MulticastCrypto, Reply, Request};

pub struct Replica {
    id: u8,
    f: usize,
    pub log: Vec<LogEntry>,
    spec_num: u32,
    multicast_signatures: HashMap<u32, MulticastSignature>,
    reorder_request: HashMap<u32, Vec<(Multicast, Request)>>,
    app: App,
    replies: HashMap<u32, Reply>,
}

enum MulticastSignature {
    SipHash(HashMap<u8, [u8; 4]>),
    P256([u8; 32], [u8; 32]),
}

#[allow(unused)]
pub struct LogEntry {
    request: Request,
    // the link hash that should appear in next multicast, if it links
    next_link: [u8; 32],
}

impl Replica {
    pub fn new(id: u8, app: App, f: usize) -> Self {
        Self {
            id,
            f,
            log: Default::default(),
            spec_num: 0,
            multicast_signatures: Default::default(),
            reorder_request: Default::default(),
            app,
            replies: Default::default(),
        }
    }
}

struct I<'a>(&'a [LogEntry]);

impl Index<u32> for I<'_> {
    type Output = LogEntry;

    fn index(&self, index: u32) -> &Self::Output {
        &self.0[(index - 1) as usize]
    }
}

type Event = dsys::NodeEvent<Message>;
type Effect = Vec<dsys::NodeEffect<Message>>;

impl Protocol<Event> for Replica {
    type Effect = Effect;

    fn update(&mut self, event: Event) -> Self::Effect {
        let Event::Handle(message) = event else {
            return Effect::NOP;
        };
        match message {
            Message::OrderedRequest(multicast, request) => self.insert_request(multicast, request),
            _ => Effect::NOP,
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
            Some(MulticastSignature::SipHash(signatures)) => signatures.len() == 3 * self.f + 1,
            Some(MulticastSignature::P256(_, _)) => true,
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

    fn insert_request(&mut self, multicast: Multicast, request: Request) -> Effect {
        if multicast.seq != self.ordered_entry() {
            self.reorder_request
                .entry(multicast.seq)
                .or_default()
                .push((multicast, request));
            return Effect::NOP;
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

        use MulticastSignature::*;
        match multicast.crypto {
            MulticastCrypto::SipHash { index, signatures } => {
                // println!("{index} {signatures:02x?}");
                if self.next_entry() > multicast.seq
                    && request != I(&self.log)[multicast.seq].request
                {
                    eprintln!("multicast request mismatch");
                    return Effect::NOP;
                }

                if self.next_entry() == multicast.seq {
                    self.log.push(LogEntry {
                        request,
                        next_link: Default::default(),
                    });
                }

                let SipHash(multicast_signatures) = self.multicast_signatures
                    .entry(multicast.seq)
                    .or_insert(SipHash(Default::default()))
                else {
                    unreachable!()
                };
                for j in index..u8::min(index + 4, (3 * self.f + 1) as _) {
                    multicast_signatures.insert(j, signatures[(j - index) as usize]);
                }
            }
            MulticastCrypto::P256 {
                link_hash: None,
                signature: Some(signature),
            } => {
                self.multicast_signatures
                    .insert(multicast.seq, P256(signature.0, signature.1));
                let next_link = I(&self.log)[multicast.seq].next_link;
                self.log.push(LogEntry {
                    request,
                    next_link: sha2::Sha256::digest(
                        &[&multicast.digest[..], &next_link[..]].concat(),
                    )
                    .into(),
                });
            }
            MulticastCrypto::P256 {
                link_hash: Some(link_hash),
                signature: None,
            } => {
                let next_link = I(&self.log)[multicast.seq].next_link;
                if link_hash != next_link {
                    eprintln!("malformed (link hash)");
                    return Effect::NOP;
                }
                self.log.push(LogEntry {
                    request,
                    next_link: sha2::Sha256::digest(
                        &[&multicast.digest[..], &next_link[..]].concat(),
                    )
                    .into(),
                });
            }
            MulticastCrypto::P256 { .. } => unreachable!(),
        }
        if !self.multicast_complete(multicast.seq) {
            // println!("incomplete");
            return Effect::NOP;
        }

        // dbg!(&request);
        // println!("complete");
        let mut effect = Effect::NOP;
        for op_num in self.spec_num + 1..=multicast.seq {
            let request = &I(&self.log)[op_num].request;
            match self.replies.get_mut(&request.client_id) {
                Some(reply) if reply.request_num > request.request_num => return Effect::NOP,
                Some(reply) if reply.request_num == request.request_num => {
                    reply.seq = multicast.seq;
                    effect = effect.compose(Effect::pure(NodeEffect::Send(
                        request.client_addr,
                        Message::Reply(reply.clone()),
                    )));
                    continue;
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
            // dbg!(&reply);
            self.replies.insert(request.client_id, reply.clone());
            effect = effect.compose(Effect::pure(NodeEffect::Send(
                request.client_addr,
                Message::Reply(reply),
            )))
        }
        self.spec_num = multicast.seq;
        effect
    }
}

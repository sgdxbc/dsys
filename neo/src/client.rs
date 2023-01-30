use std::collections::HashMap;

use bincode::Options;
use dsys::{NodeAddr, Protocol};
use sha2::Digest;

use crate::{Effect, Event, Message, Multicast, Reply, Request};

pub struct Client {
    id: u32,
    addr: NodeAddr,
    multicast_addr: NodeAddr,
    request_num: u32,
    op: Option<Box<[u8]>>,
    results: HashMap<u8, Reply>,
    ticked: u32,
    f: usize,
}

impl Client {
    pub fn new(id: u32, addr: NodeAddr, multicast_addr: NodeAddr, f: usize) -> Self {
        Self {
            id,
            addr,
            multicast_addr,
            request_num: 0,
            op: None,
            results: Default::default(),
            ticked: 0,
            f,
        }
    }
}

impl Protocol<Event> for Client {
    type Effect = Effect;

    fn init(&mut self) -> Self::Effect {
        Effect::Nop
    }

    fn update(&mut self, event: Event) -> Self::Effect {
        match event {
            Event::Op(op) => {
                assert!(self.op.is_none());
                self.op = Some(op.clone());
                self.request_num += 1;
                self.ticked = 0;
                self.do_request()
            }
            Event::Tick => {
                if self.op.is_none() {
                    return Effect::Nop;
                };
                self.ticked += 1;
                if self.ticked == 1 {
                    return Effect::Nop;
                }
                if self.ticked == 2 {
                    println!("resend");
                }
                self.do_request()
            }
            Event::Handle(Message::Reply(reply)) => {
                if self.op.is_none() || reply.request_num != self.request_num {
                    return Effect::Nop;
                }
                self.results.insert(reply.replica_id, reply.clone());
                // TODO properly check safety
                if self.results.len() == 2 * self.f + 1 {
                    self.results.drain();
                    self.op = None;
                    Effect::Notify(reply.result)
                } else {
                    Effect::Nop
                }
            }
            Event::Handle(_) => unreachable!(),
        }
    }
}

impl Client {
    fn do_request(&self) -> Effect {
        let request = Request {
            client_id: self.id,
            client_addr: self.addr,
            request_num: self.request_num,
            op: self.op.clone().unwrap(),
        };
        let request = bincode::options().serialize(&request).unwrap();
        Effect::Send(
            self.multicast_addr,
            Message::Request(Multicast {
                seq: 0,
                signature: Default::default(),
                digest: sha2::Sha256::digest(&request).into(),
                payload: request.into(),
            }),
        )
    }
}

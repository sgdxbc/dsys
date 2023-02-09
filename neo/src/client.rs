use std::collections::HashMap;

use bincode::Options;
use dsys::{
    node::{ClientEffect, ClientEvent},
    protocol::Composite,
    NodeAddr, NodeEffect, NodeEvent, Protocol,
};
use sha2::Digest;

use crate::{Message, Multicast, Reply, Request};

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

impl Protocol<ClientEvent<Message>> for Client {
    type Effect = ClientEffect<Message>;

    fn update(&mut self, event: ClientEvent<Message>) -> Self::Effect {
        match event {
            ClientEvent::Op(op) => {
                assert!(self.op.is_none());
                self.op = Some(op);
                self.request_num += 1;
                self.ticked = 0;
                self.do_request()
            }
            ClientEvent::Node(NodeEvent::Init) => ClientEffect::NOP,
            ClientEvent::Node(NodeEvent::Tick) => {
                if self.op.is_none() {
                    return ClientEffect::NOP;
                };
                self.ticked += 1;
                if self.ticked == 1 {
                    return ClientEffect::NOP;
                }
                if self.ticked == 2 {
                    eprintln!("resend");
                }
                self.do_request()
            }
            ClientEvent::Node(NodeEvent::Handle(Message::Reply(reply))) => {
                if self.op.is_none() || reply.request_num != self.request_num {
                    return ClientEffect::NOP;
                }
                self.results.insert(reply.replica_id, reply.clone());
                // TODO properly check safety
                if self.results.len() == 2 * self.f + 1 {
                    self.results.drain();
                    self.op = None;
                    ClientEffect::Result(reply.result)
                } else {
                    ClientEffect::NOP
                }
            }
            ClientEvent::Node(NodeEvent::Handle(_)) => unreachable!(),
        }
    }
}

impl Client {
    fn do_request(&self) -> ClientEffect<Message> {
        let request = Request {
            client_id: self.id,
            client_addr: self.addr,
            request_num: self.request_num,
            op: self.op.clone().unwrap(),
        };
        let digest = sha2::Sha256::digest(bincode::options().serialize(&request).unwrap()).into();
        ClientEffect::Node(NodeEffect::Send(
            self.multicast_addr,
            Message::Request(
                Multicast {
                    seq: 0,
                    signature: Default::default(),
                    digest,
                },
                request,
            ),
        ))
    }
}

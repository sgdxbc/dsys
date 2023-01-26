use std::{collections::HashMap, convert::Infallible};

use crate::{NodeAddr, NodeEffect, NodeEvent, Protocol};

#[derive(Debug, Clone)]
pub struct Request {
    client_id: u32,
    client_addr: NodeAddr,
    seq: u32,
    op: Box<[u8]>,
}

#[derive(Debug, Clone)]
pub struct Reply {
    seq: u32,
    result: Box<[u8]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResendTimeout;

pub struct Client {
    id: u32,
    addr: NodeAddr,
    replica_addr: NodeAddr,
    seq: u32,
    op: Option<Box<[u8]>>,
}

impl Protocol<NodeEvent<Reply, ResendTimeout>> for Client {
    type Effect = NodeEffect<Request, ResendTimeout>;

    fn update(&mut self, event: NodeEvent<Reply, ResendTimeout>) -> Self::Effect {
        match event {
            NodeEvent::Op(op) => {
                assert!(self.op.is_none());
                self.op = Some(op.clone());
                self.seq += 1;
                let request = Request {
                    client_id: self.id,
                    client_addr: self.addr,
                    seq: self.seq,
                    op,
                };
                NodeEffect::Send(self.replica_addr, request)
                    + NodeEffect::Set(self.addr, ResendTimeout)
            }
            NodeEvent::On(ResendTimeout) => {
                let request = Request {
                    client_id: self.id,
                    client_addr: self.addr,
                    seq: self.seq,
                    op: self.op.clone().unwrap(),
                };
                NodeEffect::Send(self.replica_addr, request)
                    + NodeEffect::Set(self.addr, ResendTimeout)
            }
            NodeEvent::Handle(reply) => {
                if self.op.is_none() || reply.seq != self.seq {
                    return NodeEffect::Nop;
                }
                self.op = None;
                NodeEffect::Notify(reply.result) + NodeEffect::Unset(self.addr, ResendTimeout)
            }
        }
    }
}

pub struct Replica<A> {
    op_number: u32,
    app: A,
    replies: HashMap<u32, Reply>,
}

impl<A> Protocol<NodeEvent<Request, Infallible>> for Replica<A>
where
    A: for<'a> Protocol<&'a [u8], Effect = Box<[u8]>>,
{
    type Effect = NodeEffect<Reply, Infallible>;

    fn update(&mut self, event: NodeEvent<Request, Infallible>) -> Self::Effect {
        let NodeEvent::Handle(request) = event else { unreachable!() };
        match self.replies.get(&request.client_id) {
            Some(reply) if reply.seq > request.seq => return NodeEffect::Nop,
            Some(reply) if reply.seq == request.seq => {
                return NodeEffect::Send(request.client_addr, reply.clone())
            }
            _ => {}
        }
        self.op_number += 1;
        let result = self.app.update(&request.op);
        let reply = Reply {
            seq: request.seq,
            result,
        };
        self.replies.insert(request.client_id, reply.clone());
        NodeEffect::Send(request.client_addr, reply)
    }
}

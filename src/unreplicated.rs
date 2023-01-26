use std::{collections::HashMap, convert::Infallible, net::SocketAddr};

use crate::{NodeEffect, Protocol};

#[derive(Debug, Clone)]
pub struct Request {
    client_id: u32,
    client_addr: SocketAddr,
    seq: u32,
    op: Box<[u8]>,
}

#[derive(Debug, Clone)]
pub struct Reply {
    seq: u32,
    result: Box<[u8]>,
}

#[derive(Debug, Clone)]
pub struct ResendTimeout;

#[derive(Debug, Clone)]
pub enum ClientEvent {
    Op(Box<[u8]>),
    Handle(Reply),
    On(ResendTimeout),
}

pub struct Client {
    id: u32,
    addr: SocketAddr,
    seq: u32,
    op: Option<Box<[u8]>>,
}

impl Protocol<ClientEvent> for Client {
    type Effect = NodeEffect<SocketAddr, Request, ResendTimeout>;

    fn update(&mut self, event: ClientEvent) -> Self::Effect {
        match event {
            ClientEvent::Op(op) => {
                assert!(self.op.is_none());
                self.op = Some(op.clone());
                self.seq += 1;
                let request = Request {
                    client_id: self.id,
                    client_addr: self.addr,
                    seq: self.seq,
                    op,
                };
                NodeEffect::Send(([0, 0, 0, 0], 0).into(), request).then(NodeEffect::Set(ResendTimeout))
            }
            ClientEvent::On(ResendTimeout) => {
                let request = Request {
                    client_id: self.id,
                    client_addr: self.addr,
                    seq: self.seq,
                    op: self.op.clone().unwrap(),
                };
                NodeEffect::Send(([0, 0, 0, 0], 0).into(), request).then(NodeEffect::Set(ResendTimeout))
            }
            ClientEvent::Handle(reply) => {
                if self.op.is_none() || reply.seq != self.seq {
                    return NodeEffect::Nop;
                }
                self.op = None;
                NodeEffect::Notify(reply.result).then(NodeEffect::Unset(ResendTimeout))
            }
        }
    }
}

pub enum ReplicaEvent {
    Handle(Request),
}

pub struct Replica<A> {
    op_number: u32,
    app: A,
    replies: HashMap<u32, Reply>,
}

impl<A> Protocol<ReplicaEvent> for Replica<A>
where
    A: for<'a> Protocol<&'a [u8], Effect = Box<[u8]>>,
{
    type Effect = NodeEffect<SocketAddr, Reply, Infallible>;

    fn update(&mut self, event: ReplicaEvent) -> Self::Effect {
        let ReplicaEvent::Handle(request) = event;
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

use std::{collections::HashMap, time::Duration};

use crate::{app::App, NodeAddr, Protocol};

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

#[derive(Debug, Clone)]
pub enum Message {
    Request(Request),
    Reply(Reply),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Timeout {
    Resend,
}

impl Timeout {
    const WAIT_RESEND: Duration = Duration::from_millis(10);
}

type Event = crate::NodeEvent<Message, Timeout>;
type Effect = crate::NodeEffect<Message, Timeout>;

pub struct Client {
    id: u32,
    addr: NodeAddr,
    replica_addr: NodeAddr,
    seq: u32,
    op: Option<Box<[u8]>>,
}

impl Client {
    pub fn new(id: u32, addr: NodeAddr, replica_addr: NodeAddr) -> Self {
        Self {
            id,
            addr,
            replica_addr,
            seq: 0,
            op: None,
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
                self.seq += 1;
                let request = Request {
                    client_id: self.id,
                    client_addr: self.addr,
                    seq: self.seq,
                    op,
                };
                Effect::Send(self.replica_addr, Message::Request(request))
                    + Effect::Set(self.addr, Timeout::Resend, Timeout::WAIT_RESEND)
            }
            Event::On(Timeout::Resend) => {
                let request = Request {
                    client_id: self.id,
                    client_addr: self.addr,
                    seq: self.seq,
                    op: self.op.clone().unwrap(),
                };
                Effect::Send(self.replica_addr, Message::Request(request))
                    + Effect::Set(self.addr, Timeout::Resend, Timeout::WAIT_RESEND)
            }
            Event::Handle(Message::Reply(reply)) => {
                if self.op.is_none() || reply.seq != self.seq {
                    return Effect::Nop;
                }
                self.op = None;
                Effect::Notify(reply.result) + Effect::Unset(self.addr, Timeout::Resend)
            }
            _ => unreachable!(),
        }
    }
}

pub struct Replica {
    op_number: u32,
    app: App,
    replies: HashMap<u32, Reply>,
}

impl Replica {
    pub fn new(app: App) -> Self {
        Self {
            op_number: 0,
            app,
            replies: Default::default(),
        }
    }
}

impl Protocol<Event> for Replica {
    type Effect = Effect;

    fn init(&mut self) -> Self::Effect {
        Effect::Nop
    }

    fn update(&mut self, event: Event) -> Self::Effect {
        let Event::Handle(Message::Request(request)) = event else { unreachable!() };
        match self.replies.get(&request.client_id) {
            Some(reply) if reply.seq > request.seq => return Effect::Nop,
            Some(reply) if reply.seq == request.seq => {
                return Effect::Send(request.client_addr, Message::Reply(reply.clone()))
            }
            _ => {}
        }
        self.op_number += 1;
        let result = self.app.execute(&request.op);
        let reply = Reply {
            seq: request.seq,
            result,
        };
        self.replies.insert(request.client_id, reply.clone());
        Effect::Send(request.client_addr, Message::Reply(reply))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        app,
        protocol::Multiplex,
        simulate::{Simulate, SimulateEffect, SimulateEvent, Workload},
        App,
        NodeAddr::{TestClient, TestReplica},
        Protocol,
    };

    use super::{Client, Message, Replica, Timeout};

    #[test]
    fn single_op() {
        let mut simulate = Simulate::<_, Message, Timeout>::default();
        simulate.nodes.insert(
            TestClient(0),
            Multiplex::A(Workload::new(
                Client::new(0, TestClient(0), TestReplica(0)),
                [b"hello".to_vec()].into_iter(),
            )),
        );
        simulate.nodes.insert(
            TestReplica(0),
            Multiplex::B(Replica::new(App::Echo(app::Echo))),
        );
        let mut effect = simulate.init();
        assert!(matches!(effect, SimulateEffect::Init));
        while {
            effect = simulate.update(SimulateEvent::Progress);
            matches!(effect, SimulateEffect::DeliverMessage)
        } {}
        assert!(matches!(effect, SimulateEffect::Halt));
        let Multiplex::A(workload) = &simulate.nodes[&TestClient(0)] else {
            unreachable!()
        };
        assert_eq!(workload.results.len(), 1);
        assert_eq!(&*workload.results[0], &b"hello"[..]);
    }
}

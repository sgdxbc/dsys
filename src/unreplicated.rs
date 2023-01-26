use std::collections::HashMap;

use crate::{NodeAddr, Protocol};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timeout {
    Resend,
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
                    + Effect::Set(self.addr, Timeout::Resend)
            }
            Event::On(Timeout::Resend) => {
                let request = Request {
                    client_id: self.id,
                    client_addr: self.addr,
                    seq: self.seq,
                    op: self.op.clone().unwrap(),
                };
                Effect::Send(self.replica_addr, Message::Request(request))
                    + Effect::Set(self.addr, Timeout::Resend)
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

pub struct Replica<A> {
    op_number: u32,
    app: A,
    replies: HashMap<u32, Reply>,
}

impl<A> Protocol<Event> for Replica<A>
where
    A: for<'a> Protocol<&'a [u8], Effect = Box<[u8]>>,
{
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
        let result = self.app.update(&request.op);
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
        network::{Network, NetworkEffect, NetworkEvent, Workload},
        protocol::Multiplex,
        NodeAddr::{TestClient, TestReplica},
        Protocol,
    };

    use super::{Client, Message, Replica, Timeout};

    struct Echo;
    impl Protocol<&'_ [u8]> for Echo {
        type Effect = Box<[u8]>;

        fn init(&mut self) -> Self::Effect {
            unreachable!()
        }

        fn update(&mut self, event: &[u8]) -> Self::Effect {
            event.to_vec().into_boxed_slice()
        }
    }

    #[test]
    fn single_op() {
        let mut network = Network::<_, Message, Timeout>::default();
        network.nodes.insert(
            TestClient(0),
            Multiplex::A(Workload::new(
                Client {
                    id: 0,
                    addr: TestClient(0),
                    replica_addr: TestReplica(0),
                    seq: 0,
                    op: None,
                },
                vec![b"hello".to_vec().into_boxed_slice()].into_iter(),
            )),
        );
        network.nodes.insert(
            TestReplica(0),
            Multiplex::B(Replica {
                op_number: 0,
                replies: Default::default(),
                app: Echo,
            }),
        );
        let mut effect = network.init();
        assert!(matches!(effect, NetworkEffect::Init));
        while {
            effect = network.update(NetworkEvent::Progress);
            matches!(effect, NetworkEffect::DeliverMessage)
        } {}
        assert!(matches!(effect, NetworkEffect::Halt));
        let Multiplex::A(workload) = &network.nodes[&TestClient(0)] else {
            unreachable!()
        };
        assert_eq!(workload.results, vec![b"hello".to_vec().into()]);
    }
}

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{app::App, NodeAddr, Protocol};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    client_id: u32,
    client_addr: NodeAddr,
    seq: u32,
    op: Box<[u8]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reply {
    seq: u32,
    result: Box<[u8]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Request(Request),
    Reply(Reply),
}

type Event = crate::NodeEvent<Message>;
type Effect = crate::NodeEffect<Message>;

pub struct Client {
    id: u32,
    addr: NodeAddr,
    replica_addr: NodeAddr,
    seq: u32,
    op: Option<Box<[u8]>>,
    ticked: u32,
}

impl Client {
    pub fn new(id: u32, addr: NodeAddr, replica_addr: NodeAddr) -> Self {
        Self {
            id,
            addr,
            replica_addr,
            seq: 0,
            op: None,
            ticked: 0,
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
                self.ticked = 0;
                let request = Request {
                    client_id: self.id,
                    client_addr: self.addr,
                    seq: self.seq,
                    op,
                };
                Effect::Send(self.replica_addr, Message::Request(request))
            }
            Event::Tick => {
                let Some(op) = &self.op else {
                    return Effect::Nop
                };
                self.ticked += 1;
                if self.ticked == 1 {
                    return Effect::Nop;
                }
                if self.ticked == 2 {
                    println!("resend");
                }
                let request = Request {
                    client_id: self.id,
                    client_addr: self.addr,
                    seq: self.seq,
                    op: op.clone(),
                };
                Effect::Send(self.replica_addr, Message::Request(request))
            }
            Event::Handle(Message::Reply(reply)) => {
                if self.op.is_none() || reply.seq != self.seq {
                    return Effect::Nop;
                }
                self.op = None;
                Effect::Notify(reply.result)
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
        let request = match event {
            Event::Handle(Message::Request(request)) => request,
            Event::Tick => return Effect::Nop,
            _ => unreachable!(),
        };
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
        node::Workload,
        protocol::Multiplex,
        simulate, App,
        NodeAddr::{TestClient, TestReplica},
        Simulate,
    };

    use super::{Client, Message, Replica};

    #[test]
    fn single_op() {
        let mut simulate = Simulate::<_, Message>::default();
        simulate.nodes.insert(
            TestClient(0),
            Multiplex::A(Workload::new_test(
                Client::new(0, TestClient(0), TestReplica(0)),
                [b"hello".to_vec()].into_iter(),
            )),
        );
        simulate.nodes.insert(
            TestReplica(0),
            Multiplex::B(Replica::new(App::Echo(app::Echo))),
        );
        simulate.init();
        let mut effect;
        while {
            effect = simulate.progress();
            matches!(effect, simulate::Progress::DeliverMessage)
        } {}
        assert!(matches!(effect, simulate::Progress::Halt));
        let Multiplex::A(workload) = &simulate.nodes[&TestClient(0)] else {
            unreachable!()
        };
        assert_eq!(workload.results.len(), 1);
        assert_eq!(&*workload.results[0], &b"hello"[..]);
    }
}

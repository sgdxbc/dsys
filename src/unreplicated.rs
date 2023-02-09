use std::{collections::HashMap, net::Ipv4Addr};

use serde::{Deserialize, Serialize};

use crate::{
    app::App,
    node::{ClientEffect, ClientEvent},
    protocol::Composite,
    NodeAddr, NodeEffect, NodeEvent, Protocol,
};

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
        assert!(!matches!(addr, NodeAddr::Socket(addr) if addr.ip() == Ipv4Addr::UNSPECIFIED));
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

impl Protocol<ClientEvent<Message>> for Client {
    type Effect = ClientEffect<Message>;

    fn update(&mut self, event: ClientEvent<Message>) -> Self::Effect {
        match event {
            ClientEvent::Op(op) => {
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
                ClientEffect::Node(NodeEffect::Send(
                    self.replica_addr,
                    Message::Request(request),
                ))
            }
            ClientEvent::Node(NodeEvent::Init) => ClientEffect::NOP,
            ClientEvent::Node(NodeEvent::Tick) => {
                let Some(op) = &self.op else {
                    return ClientEffect::NOP;
                };
                self.ticked += 1;
                if self.ticked == 1 {
                    return ClientEffect::NOP;
                }
                if self.ticked == 2 {
                    eprintln!("resend");
                }
                let request = Request {
                    client_id: self.id,
                    client_addr: self.addr,
                    seq: self.seq,
                    op: op.clone(),
                };
                ClientEffect::Node(NodeEffect::Send(
                    self.replica_addr,
                    Message::Request(request),
                ))
            }
            ClientEvent::Node(NodeEvent::Handle(Message::Reply(reply))) => {
                if self.op.is_none() || reply.seq != self.seq {
                    return ClientEffect::NOP;
                }
                self.op = None;
                ClientEffect::Result(reply.result)
            }
            _ => unreachable!(),
        }
    }
}

pub struct Replica {
    op_num: u32,
    app: App,
    replies: HashMap<u32, Reply>,
}

impl Replica {
    pub fn new(app: App) -> Self {
        Self {
            op_num: 0,
            app,
            replies: Default::default(),
        }
    }
}

impl Protocol<NodeEvent<Message>> for Replica {
    type Effect = NodeEffect<Message>;

    fn update(&mut self, event: NodeEvent<Message>) -> Self::Effect {
        let request = match event {
            NodeEvent::Handle(Message::Request(request)) => request,
            NodeEvent::Init | NodeEvent::Tick => return NodeEffect::NOP,
            _ => unreachable!(),
        };
        match self.replies.get(&request.client_id) {
            Some(reply) if reply.seq > request.seq => return NodeEffect::NOP,
            Some(reply) if reply.seq == request.seq => {
                return NodeEffect::Send(request.client_addr, Message::Reply(reply.clone()))
            }
            _ => {}
        }
        self.op_num += 1;
        let result = self.app.execute(&request.op);
        let reply = Reply {
            seq: request.seq,
            result,
        };
        self.replies.insert(request.client_id, reply.clone());
        NodeEffect::Send(request.client_addr, Message::Reply(reply))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        app,
        node::Workload,
        protocol::OneOf,
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
            OneOf::A(Workload::new_test(
                Client::new(0, TestClient(0), TestReplica(0)),
                [b"hello".to_vec()].into_iter(),
            )),
        );
        simulate
            .nodes
            .insert(TestReplica(0), OneOf::B(Replica::new(App::Echo(app::Echo))));
        simulate.init();
        let mut effect;
        while {
            effect = simulate.progress();
            matches!(effect, simulate::Progress::DeliverMessage)
        } {}
        assert!(matches!(effect, simulate::Progress::Halt));
        let OneOf::A(workload) = &simulate.nodes[&TestClient(0)] else {
            unreachable!()
        };
        assert_eq!(workload.results.len(), 1);
        assert_eq!(&*workload.results[0], &b"hello"[..]);
    }
}

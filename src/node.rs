use std::{net::SocketAddr, ops::Add, time::Duration};

use crate::Protocol;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeAddr {
    TestClient(u32),
    TestReplica(u32),
    Socket(SocketAddr),
}

#[derive(Debug)]
pub enum NodeEvent<M, T> {
    Handle(M),
    On(T),
    Op(Box<[u8]>),
}

#[derive(Debug)]
pub enum NodeEffect<M, T> {
    Nop,
    Notify(Box<[u8]>),
    Send(NodeAddr, M),
    Set(NodeAddr, T, Duration),
    Reset(NodeAddr, T, Duration),
    Unset(NodeAddr, T),
    Compose(Vec<NodeEffect<M, T>>),
}

impl<M, T> Add for NodeEffect<M, T> {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        match (self, other) {
            (Self::Nop, Self::Nop) => Self::Nop,
            (Self::Nop, effect) | (effect, Self::Nop) => effect,
            (Self::Compose(mut effects), Self::Compose(other_effects)) => {
                effects.extend(other_effects);
                Self::Compose(effects)
            }
            (Self::Compose(mut effects), effect) | (effect, Self::Compose(mut effects)) => {
                effects.push(effect);
                Self::Compose(effects)
            }
            (effect, other_effect) => Self::Compose(vec![effect, other_effect]),
        }
    }
}

pub struct Workload<N, I> {
    node: N,
    ops: I,
    pub results: Vec<Box<[u8]>>,
}

impl<N, I> Workload<N, I> {
    pub fn new(node: N, ops: I) -> Self {
        Self {
            node,
            ops,
            results: Default::default(),
        }
    }

    fn work<M, T, O>(&mut self) -> NodeEffect<M, T>
    where
        N: Protocol<NodeEvent<M, T>, Effect = NodeEffect<M, T>>,
        I: Iterator<Item = O>,
        O: Into<Box<[u8]>>,
    {
        if let Some(op) = self.ops.next() {
            self.node.update(NodeEvent::Op(op.into()))
        } else {
            NodeEffect::Nop
        }
    }

    fn process_effect<M, T, O>(&mut self, effect: NodeEffect<M, T>) -> NodeEffect<M, T>
    where
        N: Protocol<NodeEvent<M, T>, Effect = NodeEffect<M, T>>,
        I: Iterator<Item = O>,
        O: Into<Box<[u8]>>,
    {
        match effect {
            NodeEffect::Notify(result) => {
                self.results.push(result);
                self.work()
            }
            NodeEffect::Compose(mut effects) => {
                if effects.is_empty() {
                    NodeEffect::Nop
                } else {
                    let effect = effects.pop().unwrap();
                    self.process_effect(effect) + self.process_effect(NodeEffect::Compose(effects))
                }
            }
            effect => effect,
        }
    }
}

impl<N, I, O, M, T> Protocol<NodeEvent<M, T>> for Workload<N, I>
where
    N: Protocol<NodeEvent<M, T>, Effect = NodeEffect<M, T>>,
    I: Iterator<Item = O>,
    O: Into<Box<[u8]>>,
{
    type Effect = NodeEffect<M, T>;

    fn init(&mut self) -> Self::Effect {
        self.work()
    }

    fn update(&mut self, event: NodeEvent<M, T>) -> Self::Effect {
        let effect = self.node.update(event);
        self.process_effect(effect)
    }
}

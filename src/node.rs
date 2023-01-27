use std::{
    net::SocketAddr,
    ops::Add,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use crate::Protocol;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeAddr {
    TestClient(u32),
    TestReplica(u32),
    Socket(SocketAddr),
}

#[derive(Debug)]
pub enum NodeEvent<M> {
    Handle(M),
    Op(Box<[u8]>),
    Tick,
}

#[derive(Debug)]
pub enum NodeEffect<M> {
    Nop,
    Notify(Box<[u8]>),
    Send(NodeAddr, M),
    Compose(Vec<NodeEffect<M>>),
}

impl<M> Add for NodeEffect<M> {
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
    benchmark: bool,
    instant: Instant,
    pub latencies: Vec<Duration>,
    pub count: Arc<AtomicU32>,
}

impl<N, I> Workload<N, I> {
    pub fn new(node: N, ops: I) -> Self {
        Self {
            node,
            ops,
            results: Default::default(),
            benchmark: false,
            instant: Instant::now(),
            latencies: Default::default(),
            count: Default::default(),
        }
    }

    pub fn new_benchmark(node: N, ops: I, count: Arc<AtomicU32>) -> Self {
        Self {
            node,
            ops,
            results: Default::default(),
            instant: Instant::now(),
            benchmark: true,
            latencies: Default::default(),
            count,
        }
    }

    fn work<M, O>(&mut self) -> NodeEffect<M>
    where
        N: Protocol<NodeEvent<M>, Effect = NodeEffect<M>>,
        I: Iterator<Item = O>,
        O: Into<Box<[u8]>>,
    {
        if let Some(op) = self.ops.next() {
            self.instant = Instant::now();
            self.node.update(NodeEvent::Op(op.into()))
        } else {
            NodeEffect::Nop
        }
    }

    fn process_effect<M, O>(&mut self, effect: NodeEffect<M>) -> NodeEffect<M>
    where
        N: Protocol<NodeEvent<M>, Effect = NodeEffect<M>>,
        I: Iterator<Item = O>,
        O: Into<Box<[u8]>>,
    {
        match effect {
            NodeEffect::Notify(result) => {
                if self.benchmark {
                    self.latencies.push(Instant::now() - self.instant);
                    self.count.fetch_add(1, Ordering::SeqCst);
                } else {
                    self.results.push(result);
                }
                // TODO able to throttle
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

impl<N, I, O, M> Protocol<NodeEvent<M>> for Workload<N, I>
where
    N: Protocol<NodeEvent<M>, Effect = NodeEffect<M>>,
    I: Iterator<Item = O>,
    O: Into<Box<[u8]>>,
{
    type Effect = NodeEffect<M>;

    fn init(&mut self) -> Self::Effect {
        self.work()
    }

    fn update(&mut self, event: NodeEvent<M>) -> Self::Effect {
        let effect = self.node.update(event);
        self.process_effect(effect)
    }
}

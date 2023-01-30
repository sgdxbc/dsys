use std::{
    net::SocketAddr,
    ops::Add,
    sync::{
        atomic::{AtomicU8, Ordering},
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
    instant: Instant,
    pub latencies: Vec<Duration>,
    pub mode: Arc<AtomicU8>,
}

pub enum WorkloadMode {
    Discard,
    Test,
    Benchmark,
}

impl WorkloadMode {
    const DISCARD: u8 = Self::Discard as _;
    const TEST: u8 = Self::Test as _;
    const BENCHMARK: u8 = Self::Benchmark as _;
}

impl<N, I> Workload<N, I> {
    pub fn new_test(node: N, ops: I) -> Self {
        Self {
            node,
            ops,
            results: Default::default(),
            instant: Instant::now(),
            latencies: Default::default(),
            mode: Arc::new(AtomicU8::new(WorkloadMode::Test as _)),
        }
    }

    pub fn new_benchmark(node: N, ops: I, mode: Arc<AtomicU8>) -> Self {
        Self {
            node,
            ops,
            results: Default::default(),
            instant: Instant::now(),
            latencies: Default::default(),
            mode,
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
                match self.mode.load(Ordering::SeqCst) {
                    WorkloadMode::DISCARD => {}
                    WorkloadMode::TEST => self.results.push(result),
                    WorkloadMode::BENCHMARK => self.latencies.push(Instant::now() - self.instant),
                    _ => unreachable!(),
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

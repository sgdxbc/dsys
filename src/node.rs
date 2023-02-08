use std::{
    mem::replace,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use crossbeam::channel;
use serde::{Deserialize, Serialize};

use crate::{
    protocol::{Composite, Generate},
    Protocol,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeAddr {
    TestClient(u32),
    TestReplica(u32),
    Socket(SocketAddr),
}

#[derive(Debug)]
pub enum NodeEvent<M> {
    Init,
    Handle(M),
    Tick,
}

#[derive(Debug)]
pub enum NodeEffect<M> {
    Nop,
    Send(NodeAddr, M),
    Broadcast(M),
    Compose(Vec<NodeEffect<M>>),
}

impl<M> Composite for NodeEffect<M> {
    const NOP: Self = Self::Nop;

    fn compose(self, other: Self) -> Self {
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

    fn decompose(&mut self) -> Option<Self> {
        match self {
            Self::Nop => None,
            Self::Compose(effects) if effects.len() > 1 => Some(effects.pop().unwrap()),
            _ => Some(replace(self, Self::Nop)),
        }
    }
}

pub enum ClientEvent<M> {
    Op(Box<[u8]>),
    Node(NodeEvent<M>),
}

pub enum ClientEffect<M> {
    Result(Box<[u8]>),
    Node(NodeEffect<M>),
}

impl<M> Composite for ClientEffect<M> {
    const NOP: Self = Self::Node(NodeEffect::NOP);

    fn compose(self, other: Self) -> Self {
        match (self, other) {
            (Self::Result(result), Self::Node(NodeEffect::Nop))
            | (Self::Node(NodeEffect::Nop), Self::Result(result)) => Self::Result(result),
            (Self::Node(NodeEffect::Nop), Self::Node(NodeEffect::Nop)) => {
                Self::Node(NodeEffect::Nop)
            }
            _ => panic!(),
        }
    }

    fn decompose(&mut self) -> Option<Self> {
        match self {
            Self::Result(_) => Some(replace(self, Self::Node(NodeEffect::Nop))),
            Self::Node(effect) => effect.decompose().map(Self::Node),
        }
    }
}

pub struct Lifecycle<M> {
    event_channel: channel::Receiver<NodeEvent<M>>,
    running: Arc<AtomicBool>,
}

impl<M> Lifecycle<M> {
    pub fn new(event_channel: channel::Receiver<NodeEvent<M>>, running: Arc<AtomicBool>) -> Self {
        Self {
            event_channel,
            running,
        }
    }
}

impl<M> Generate for Lifecycle<M> {
    type Event<'a> = NodeEvent<M>;

    fn deploy<P>(&mut self, node: &mut P)
    where
        P: for<'a> Protocol<Self::Event<'a>>,
    {
        assert!(!self.running.swap(true, Ordering::SeqCst));

        node.update(NodeEvent::Init);

        let mut deadline = Instant::now() + Duration::from_millis(10);
        while self.running.load(Ordering::SeqCst) {
            match self.event_channel.recv_deadline(deadline) {
                Ok(event) => {
                    node.update(event);
                }
                Err(channel::RecvTimeoutError::Disconnected) => break,
                Err(channel::RecvTimeoutError::Timeout) => {
                    deadline = Instant::now() + Duration::from_millis(10);
                    node.update(NodeEvent::Tick);
                }
            }
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
        N: Protocol<ClientEvent<M>, Effect = ClientEffect<M>>,
        I: Iterator<Item = O>,
        O: Into<Box<[u8]>>,
    {
        if let Some(op) = self.ops.next() {
            self.instant = Instant::now();
            if let ClientEffect::Node(effect) = self.node.update(ClientEvent::Op(op.into())) {
                effect
            } else {
                panic!()
            }
        } else {
            // record finished?
            NodeEffect::Nop
        }
    }

    fn process_effect<M, O>(&mut self, effect: ClientEffect<M>) -> NodeEffect<M>
    where
        N: Protocol<ClientEvent<M>, Effect = ClientEffect<M>>,
        I: Iterator<Item = O>,
        O: Into<Box<[u8]>>,
    {
        match effect {
            ClientEffect::Result(result) => {
                match self.mode.load(Ordering::SeqCst) {
                    WorkloadMode::DISCARD => {}
                    WorkloadMode::TEST => self.results.push(result),
                    WorkloadMode::BENCHMARK => self.latencies.push(Instant::now() - self.instant),
                    _ => unreachable!(),
                }
                // TODO able to throttle
                self.work()
            }
            ClientEffect::Node(effect) => effect,
        }
    }
}

impl<N, I, O, M> Protocol<NodeEvent<M>> for Workload<N, I>
where
    N: Protocol<ClientEvent<M>, Effect = ClientEffect<M>>,
    I: Iterator<Item = O>,
    O: Into<Box<[u8]>>,
{
    type Effect = NodeEffect<M>;

    fn update(&mut self, event: NodeEvent<M>) -> Self::Effect {
        let is_init = matches!(event, NodeEvent::Init);
        let effect = self.node.update(ClientEvent::Node(event));
        let mut effect = self.process_effect(effect);
        if is_init {
            effect = effect.compose(self.work());
        }
        effect
    }
}

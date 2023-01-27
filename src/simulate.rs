use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    hash::Hash,
};

use crate::{NodeAddr, NodeEffect, NodeEvent, Protocol};

pub enum SimulateEvent {
    Progress,
    Filter, // TODO
}

pub enum SimulateEffect {
    DeliverMessage,
    DeliverTimeout(u64),
    Init,
    Halt,
}

pub struct Simulate<N, M, T> {
    pub nodes: HashMap<NodeAddr, N>,
    messages: VecDeque<(NodeAddr, M)>,
    now_millis: u64,
    timeout_millis: BTreeMap<u64, (NodeAddr, T)>,
    timeouts: HashMap<(NodeAddr, T), u64>,
}

impl<N, M, T> Default for Simulate<N, M, T> {
    fn default() -> Self {
        Self {
            nodes: Default::default(),
            messages: Default::default(),
            now_millis: 0,
            timeout_millis: Default::default(),
            timeouts: Default::default(),
        }
    }
}

impl<N, M, T> Protocol<SimulateEvent> for Simulate<N, M, T>
where
    N: Protocol<NodeEvent<M, T>, Effect = NodeEffect<M, T>>,
    T: Eq + Hash + Clone,
{
    type Effect = SimulateEffect;

    fn init(&mut self) -> Self::Effect {
        for effect in self
            .nodes
            .values_mut()
            .map(|node| node.init())
            .collect::<Vec<_>>()
        {
            self.push_effect(effect)
        }
        SimulateEffect::Init
    }

    fn update(&mut self, event: SimulateEvent) -> Self::Effect {
        match event {
            SimulateEvent::Filter => todo!(),
            SimulateEvent::Progress => {
                if let Some((destination, message)) = self.messages.pop_front() {
                    let effect = self
                        .nodes
                        .get_mut(&destination)
                        .unwrap()
                        .update(NodeEvent::Handle(message));
                    self.push_effect(effect);
                    return SimulateEffect::DeliverMessage;
                }
                if let Some((now, (destination, timeout))) = self.timeout_millis.pop_first() {
                    self.timeouts.remove(&(destination, timeout.clone()));
                    self.now_millis = now;
                    let effect = self
                        .nodes
                        .get_mut(&destination)
                        .unwrap()
                        .update(NodeEvent::On(timeout));
                    self.push_effect(effect);
                    return SimulateEffect::DeliverTimeout(self.now_millis);
                }
                SimulateEffect::Halt
            }
        }
    }
}

impl<N, M, T> Simulate<N, M, T> {
    fn push_effect(&mut self, effect: NodeEffect<M, T>)
    where
        T: Eq + Hash + Clone,
    {
        match effect {
            NodeEffect::Compose(effects) => {
                for effect in effects {
                    self.push_effect(effect)
                }
            }
            NodeEffect::Nop => {}
            NodeEffect::Notify(_) => unreachable!(),
            NodeEffect::Send(address, message) => self.messages.push_back((address, message)),
            NodeEffect::Set(address, timeout, wait) => {
                let at = self.now_millis + wait.as_millis() as u64;
                self.timeout_millis.insert(at, (address, timeout.clone()));
                self.timeouts.insert((address, timeout), at);
            }
            NodeEffect::Unset(address, timeout) => {
                let at = self.timeouts.remove(&(address, timeout)).unwrap();
                self.timeout_millis.remove(&at);
            }
            NodeEffect::Reset(address, timeout, wait) => {
                self.push_effect(NodeEffect::Unset(address, timeout.clone()));
                self.push_effect(NodeEffect::Set(address, timeout, wait));
            }
        }
    }
}

pub struct Workload<N, I = <Vec<Vec<u8>> as IntoIterator>::IntoIter> {
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

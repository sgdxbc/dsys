use std::collections::{HashMap, VecDeque};

use crate::{NodeAddr, NodeEffect, NodeEvent, Protocol};

pub enum NetworkEvent {
    Progress,
    Filter, // TODO
}

pub enum NetworkEffect {
    DeliverMessage,
    DeliverTimeout,
    Init,
    Halt,
}

pub struct Network<N, M, T> {
    pub nodes: HashMap<NodeAddr, N>,
    messages: VecDeque<(NodeAddr, M)>,
    timeouts: VecDeque<(NodeAddr, T)>,
}

impl<N, M, T> Default for Network<N, M, T> {
    fn default() -> Self {
        Self {
            nodes: Default::default(),
            messages: Default::default(),
            timeouts: Default::default(),
        }
    }
}

impl<N, M, T> Protocol<NetworkEvent> for Network<N, M, T>
where
    N: Protocol<NodeEvent<M, T>, Effect = NodeEffect<M, T>>,
    T: Eq,
{
    type Effect = NetworkEffect;

    fn init(&mut self) -> Self::Effect {
        for effect in self
            .nodes
            .values_mut()
            .map(|node| node.init())
            .collect::<Vec<_>>()
        {
            self.push_effect(effect)
        }
        NetworkEffect::Init
    }

    fn update(&mut self, event: NetworkEvent) -> Self::Effect {
        match event {
            NetworkEvent::Filter => todo!(),
            NetworkEvent::Progress => {
                if let Some((destination, message)) = self.messages.pop_front() {
                    let effect = self
                        .nodes
                        .get_mut(&destination)
                        .unwrap()
                        .update(NodeEvent::Handle(message));
                    self.push_effect(effect);
                    return NetworkEffect::DeliverMessage;
                }
                if let Some((destination, timeout)) = self.timeouts.pop_front() {
                    let effect = self
                        .nodes
                        .get_mut(&destination)
                        .unwrap()
                        .update(NodeEvent::On(timeout));
                    self.push_effect(effect);
                    return NetworkEffect::DeliverTimeout;
                }
                NetworkEffect::Halt
            }
        }
    }
}

impl<N, M, T> Network<N, M, T> {
    fn push_effect(&mut self, effect: NodeEffect<M, T>)
    where
        T: Eq,
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
            NodeEffect::Set(address, timeout) => self.timeouts.push_back((address, timeout)),
            NodeEffect::Unset(address, timeout) => {
                let i = self
                    .timeouts
                    .iter()
                    .position(|t| (t.0, &t.1) == (address, &timeout))
                    .unwrap();
                self.timeouts.remove(i);
            }
            NodeEffect::Reset(address, timeout) => {
                let i = self
                    .timeouts
                    .iter()
                    .position(|t| (t.0, &t.1) == (address, &timeout))
                    .unwrap();
                self.timeouts.swap(i, self.timeouts.len() - 1);
            }
        }
    }
}

pub struct Workload<N, O = <Vec<Box<[u8]>> as IntoIterator>::IntoIter> {
    node: N,
    ops: O,
    pub results: Vec<Box<[u8]>>,
}

impl<N, O> Workload<N, O> {
    pub fn new(node: N, ops: O) -> Self {
        Self {
            node,
            ops,
            results: Default::default(),
        }
    }

    fn work<M, T>(&mut self) -> NodeEffect<M, T>
    where
        N: Protocol<NodeEvent<M, T>, Effect = NodeEffect<M, T>>,
        O: Iterator<Item = Box<[u8]>>,
    {
        if let Some(op) = self.ops.next() {
            self.node.update(NodeEvent::Op(op))
        } else {
            NodeEffect::Nop
        }
    }

    fn process_effect<M, T>(&mut self, effect: NodeEffect<M, T>) -> NodeEffect<M, T>
    where
        N: Protocol<NodeEvent<M, T>, Effect = NodeEffect<M, T>>,
        O: Iterator<Item = Box<[u8]>>,
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

impl<N, O, M, T> Protocol<NodeEvent<M, T>> for Workload<N, O>
where
    N: Protocol<NodeEvent<M, T>, Effect = NodeEffect<M, T>>,
    O: Iterator<Item = Box<[u8]>>,
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

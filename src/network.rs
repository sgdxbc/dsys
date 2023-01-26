use std::collections::{HashMap, VecDeque};

use crate::{NodeAddr, NodeEffect, NodeEvent, Protocol};

pub enum NetworkEvent {
    Progress,
    Filter, // TODO
}

pub enum NetworkEffect {
    DeliverMessage,
    DeliverTimeout,
    Halt,
}

pub struct Network<N, M, T> {
    nodes: HashMap<NodeAddr, N>,
    messages: VecDeque<(NodeAddr, M)>,
    timeouts: VecDeque<(NodeAddr, T)>,
}

impl<N, M, T> Protocol<NetworkEvent> for Network<N, M, T>
where
    N: Protocol<NodeEvent<M, T>, Effect = NodeEffect<M, T>>,
    T: Eq,
{
    type Effect = NetworkEffect;

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
    results: Vec<Box<[u8]>>,
}

impl<N, O, M, T> Protocol<NodeEvent<M, T>> for Workload<N, O>
where
    N: Protocol<NodeEvent<M, T>, Effect = NodeEffect<M, T>>,
    O: Iterator<Item = Box<[u8]>>,
{
    type Effect = NodeEffect<M, T>;

    fn update(&mut self, event: NodeEvent<M, T>) -> Self::Effect {
        match self.node.update(event) {
            NodeEffect::Notify(result) => {
                self.results.push(result);
                if let Some(op) = self.ops.next() {
                    self.node.update(NodeEvent::Op(op))
                } else {
                    NodeEffect::Nop
                }
            }
            effect => effect,
        }
    }
}

use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    hash::Hash,
};

use crate::{NodeAddr, NodeEffect, NodeEvent, Protocol};

pub enum Progress {
    DeliverMessage,
    DeliverTimeout(u64),
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

impl<N, M, T> Simulate<N, M, T> {
    pub fn init(&mut self)
    where
        N: Protocol<NodeEvent<M, T>, Effect = NodeEffect<M, T>>,
        T: Eq + Hash + Clone,
    {
        for effect in self
            .nodes
            .values_mut()
            .map(|node| node.init())
            .collect::<Vec<_>>()
        {
            self.push_effect(effect)
        }
    }

    pub fn progress(&mut self) -> Progress
    where
        N: Protocol<NodeEvent<M, T>, Effect = NodeEffect<M, T>>,
        T: Eq + Hash + Clone,
    {
        if let Some((destination, message)) = self.messages.pop_front() {
            let effect = self
                .nodes
                .get_mut(&destination)
                .unwrap()
                .update(NodeEvent::Handle(message));
            self.push_effect(effect);
            return Progress::DeliverMessage;
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
            return Progress::DeliverTimeout(self.now_millis);
        }
        Progress::Halt
    }

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

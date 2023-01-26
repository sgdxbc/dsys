use std::{net::SocketAddr, ops::Add};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeAddr {
    TestClient(u32),
    TestReplica(u32),
    Socket(SocketAddr),
}

pub enum NodeEvent<M, T> {
    Op(Box<[u8]>),
    Handle(M),
    On(T),
}

pub enum NodeEffect<M, T> {
    Nop,
    Notify(Box<[u8]>),
    Send(NodeAddr, M),
    Set(NodeAddr, T),
    Reset(NodeAddr, T),
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

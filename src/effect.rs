#![allow(unused)]

use crate::protocol::FeedbackEffect;

pub enum NodeEffect<A, M, T> {
    Nop,
    Notify(Box<[u8]>),
    Send(A, M),
    Set(T),
    Reset(T),
    Unset(T),
    Then(Vec<NodeEffect<A, M, T>>),
}

impl<A, M, T> NodeEffect<A, M, T> {
    pub fn then(self, other: Self) -> Self {
        match (self, other) {
            (Self::Nop, Self::Nop) => Self::Nop,
            (Self::Nop, effect) | (effect, Self::Nop) => effect,
            (Self::Then(mut effects), Self::Then(other_effects)) => {
                effects.extend(other_effects);
                Self::Then(effects)
            }
            (Self::Then(mut effects), effect) | (effect, Self::Then(mut effects)) => {
                effects.push(effect);
                Self::Then(effects)
            }
            (effect, other_effect) => Self::Then(vec![effect, other_effect]),
        }
    }
}

impl<I, M, T> From<NodeEffect<I, M, T>> for FeedbackEffect<NodeEffect<I, M, T>> {
    fn from(effect: NodeEffect<I, M, T>) -> Self {
        if matches!(effect, NodeEffect::Notify(_) | NodeEffect::Nop) {
            Self::External(effect)
        } else {
            Self::Internal(effect)
        }
    }
}

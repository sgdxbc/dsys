use std::thread::JoinHandle;

use crossbeam::channel;
use nix::{
    sched::{sched_setaffinity, CpuSet},
    unistd::Pid,
};

pub trait Protocol<Event> {
    type Effect;

    fn update(&mut self, event: Event) -> Self::Effect;
}

pub trait Init {
    const INIT: Self;
}

pub trait Composite: Sized {
    const NOP: Self;
    fn compose(self, other: Self) -> Self;
    fn decompose(&mut self) -> Option<Self>;
}

impl Composite for () {
    const NOP: Self = ();

    fn compose(self, _: Self) -> Self {}

    fn decompose(&mut self) -> Option<Self> {
        None
    }
}

pub fn spawn<P, E>(
    protocols: Box<[P]>,
    affinity: Option<usize>,
) -> (
    Box<[JoinHandle<P>]>,
    channel::Sender<E>,
    channel::Receiver<P::Effect>,
)
where
    P: Protocol<E> + Send + 'static,
    E: Send + 'static,
    P::Effect: Send + 'static,
{
    let event_channel = channel::unbounded();
    let effect_channel = channel::unbounded();
    let mut handles = Vec::new();
    for (i, mut protocol) in Vec::from(protocols).into_iter().enumerate() {
        let event_channel = event_channel.1.clone();
        let effect_channel = effect_channel.0.clone();
        let handle = std::thread::spawn(move || {
            if let Some(affinity) = affinity {
                let mut cpu_set = CpuSet::new();
                cpu_set.set(affinity + i).unwrap();
                sched_setaffinity(Pid::from_raw(0), &cpu_set).unwrap();
            }
            for event in event_channel.iter() {
                effect_channel.send(protocol.update(event)).unwrap(); //
            }
            protocol
        });
        handles.push(handle);
    }
    (handles.into(), event_channel.0, effect_channel.1)
}

pub enum Multiplex<A, B> {
    A(A),
    B(B),
}

impl<A, B, E> Protocol<E> for Multiplex<A, B>
where
    A: Protocol<E>,
    B: Protocol<E, Effect = A::Effect>,
{
    type Effect = A::Effect;

    fn update(&mut self, event: E) -> Self::Effect {
        match self {
            Multiplex::A(protocol) => protocol.update(event),
            Multiplex::B(protocol) => protocol.update(event),
        }
    }
}

pub struct Then<A, B>(A, B);

impl<A, B, E> Protocol<E> for Then<A, B>
where
    A: Protocol<E>,
    B: Protocol<A::Effect>,
    B::Effect: Composite,
{
    type Effect = B::Effect;

    fn update(&mut self, event: E) -> Self::Effect {
        self.1.update(self.0.update(event))
    }
}

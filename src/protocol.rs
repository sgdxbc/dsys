use std::thread::JoinHandle;

use crossbeam::channel;

use crate::set_affinity;

pub trait Protocol<Event> {
    type Effect;

    fn update(&mut self, event: Event) -> Self::Effect;

    fn then<P>(self, other: P) -> Then<Self, P>
    where
        Self: Sized,
        P: Protocol<Self::Effect>,
    {
        Then(self, other)
    }
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

pub trait Generate {
    type Event;

    fn deploy<P>(&mut self, protocol: &mut P, channel: Option<channel::Sender<P::Effect>>)
    where
        P: Protocol<Self::Event>;
}

impl<E> Generate for channel::Receiver<E> {
    type Event = E;

    fn deploy<P>(&mut self, protocol: &mut P, channel: Option<channel::Sender<P::Effect>>)
    where
        P: Protocol<Self::Event>,
    {
        for event in self.iter() {
            if let Some(channel) = &channel {
                channel.send(protocol.update(event)).unwrap()
            }
        }
    }
}

pub fn spawn<P, E>(
    protocols: Box<[P]>,
    affinity: Option<usize>,
    event_channel: channel::Receiver<E>,
    effect_channel: Option<channel::Sender<P::Effect>>,
) -> Box<[JoinHandle<P>]>
where
    P: Protocol<E> + Send + 'static,
    E: Send + 'static,
    P::Effect: Send + 'static,
{
    let mut handles = Vec::new();
    for (i, mut protocol) in Vec::from(protocols).into_iter().enumerate() {
        let event_channel = event_channel.clone();
        let effect_channel = effect_channel.clone();
        let handle = std::thread::spawn(move || {
            set_affinity(affinity.map(|n| n + i));
            for event in event_channel.iter() {
                let effect = protocol.update(event);
                if let Some(effect_channel) = &effect_channel {
                    effect_channel.send(effect).unwrap(); //
                }
            }
            protocol
        });
        handles.push(handle);
    }
    handles.into()
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
{
    type Effect = B::Effect;

    fn update(&mut self, event: E) -> Self::Effect {
        self.1.update(self.0.update(event))
    }
}

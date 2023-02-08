use crossbeam::channel;

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

impl<P, E> Protocol<E> for &mut P
where
    P: Protocol<E>,
{
    type Effect = P::Effect;

    fn update(&mut self, event: E) -> Self::Effect {
        P::update(self, event)
    }
}

pub struct Identity;
impl<E> Protocol<E> for Identity {
    type Effect = E;

    fn update(&mut self, event: E) -> Self::Effect {
        event
    }
}

pub struct Null;
impl<E> Protocol<E> for Null {
    type Effect = ();

    fn update(&mut self, _: E) -> Self::Effect {}
}

impl<E> Protocol<E> for channel::Sender<E> {
    type Effect = ();

    fn update(&mut self, event: E) -> Self::Effect {
        self.send(event).unwrap()
    }
}

pub trait Generate {
    type Event<'a>;

    fn deploy<P>(&mut self, protocol: &mut P)
    where
        P: for<'a> Protocol<Self::Event<'a>, Effect = ()>;
}

impl<E> Generate for channel::Receiver<E> {
    type Event<'a> = E;

    fn deploy<P>(&mut self, protocol: &mut P)
    where
        P: for<'a> Protocol<Self::Event<'a>>,
    {
        for event in self.iter() {
            protocol.update(event);
        }
    }
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

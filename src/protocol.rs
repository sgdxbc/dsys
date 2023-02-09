use crossbeam::{channel, select};

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

    fn each_then<P>(self, other: P) -> EachThen<Self, P>
    where
        Self: Sized,
        P: Protocol<Self::Effect>,
    {
        EachThen(self, other)
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

pub struct Map<F>(pub F);
impl<E, F, T> Protocol<E> for Map<F>
where
    F: FnMut(E) -> T,
{
    type Effect = T;

    fn update(&mut self, event: E) -> Self::Effect {
        (self.0)(event)
    }
}

impl<E> Protocol<E> for channel::Sender<E> {
    type Effect = ();

    fn update(&mut self, event: E) -> Self::Effect {
        self.send(event).unwrap()
    }
}

impl<E> Protocol<Option<E>> for channel::Sender<E> {
    type Effect = ();

    fn update(&mut self, event: Option<E>) -> Self::Effect {
        if let Some(event) = event {
            self.send(event).unwrap()
        }
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

pub enum OneOf<A, B> {
    A(A),
    B(B),
}

impl<A, B, E> Protocol<E> for OneOf<A, B>
where
    A: Protocol<E>,
    B: Protocol<E, Effect = A::Effect>,
{
    type Effect = A::Effect;

    fn update(&mut self, event: E) -> Self::Effect {
        match self {
            OneOf::A(protocol) => protocol.update(event),
            OneOf::B(protocol) => protocol.update(event),
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

pub struct EachThen<A, B>(A, B);

impl<A, B, E> Protocol<E> for EachThen<A, B>
where
    A: Protocol<E>,
    B: Protocol<A::Effect>,
    A::Effect: Composite,
    B::Effect: Composite,
{
    type Effect = B::Effect;

    fn update(&mut self, event: E) -> Self::Effect {
        let mut effect_a = self.0.update(event);
        let mut effect_b = B::Effect::NOP;
        while let Some(basic_effect) = effect_a.decompose() {
            effect_b = effect_b.compose(self.1.update(basic_effect));
        }
        effect_b
    }
}

pub enum Multiplex<A, B> {
    A(A),
    B(B),
}

impl<A, B, EventA, EventB> Protocol<Multiplex<EventA, EventB>> for (A, B)
where
    A: Protocol<EventA>,
    B: Protocol<EventB>,
{
    type Effect = Multiplex<A::Effect, B::Effect>;

    fn update(&mut self, event: Multiplex<EventA, EventB>) -> Self::Effect {
        match event {
            Multiplex::A(event) => Multiplex::A(self.0.update(event)),
            Multiplex::B(event) => Multiplex::B(self.1.update(event)),
        }
    }
}

impl From<Multiplex<(), ()>> for () {
    fn from(_: Multiplex<(), ()>) -> Self {}
}

impl<A, B> Generate for (channel::Receiver<A>, channel::Receiver<B>) {
    type Event<'a> = Multiplex<A, B>;

    fn deploy<P>(&mut self, protocol: &mut P)
    where
        P: for<'a> Protocol<Self::Event<'a>, Effect = ()>,
    {
        let mut disconnected = (false, false);
        while {
            select! {
                recv(self.0) -> event => if let Ok(event) = event {
                    protocol.update(Multiplex::A(event))
                } else {
                    disconnected.0 = true;
                },
                recv(self.1) -> event => if let Ok(event) = event {
                    protocol.update(Multiplex::B(event))
                } else {
                    disconnected.1 = true;
                },
            }
            disconnected == (true, true)
        } {}
    }
}

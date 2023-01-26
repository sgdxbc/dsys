pub trait Protocol<Event> {
    type Effect;

    fn update(&mut self, event: Event) -> Self::Effect;

    fn feedback<P, E>(self, other: P) -> Feedback<Self, P>
    where
        Self: Sized,
        P: Protocol<E>,
    {
        Feedback(self, other)
    }

    fn batch(self) -> Batch<Self>
    where
        Self: Sized,
    {
        Batch(self)
    }
}

pub enum Multiplex<A, B> {
    A(A),
    B(B),
}

pub enum MultiplexEvent<A, B> {
    A(A),
    B(B),
}

pub enum MultiplexEffect<A, B> {
    A(A),
    B(B),
}

impl<A, B, EventA, EventB> Protocol<MultiplexEvent<EventA, EventB>> for Multiplex<A, B>
where
    A: Protocol<EventA>,
    B: Protocol<EventB>,
{
    type Effect = MultiplexEffect<A::Effect, B::Effect>;

    fn update(&mut self, event: MultiplexEvent<EventA, EventB>) -> Self::Effect {
        match (self, event) {
            (Multiplex::A(protocol), MultiplexEvent::A(event)) => {
                MultiplexEffect::A(protocol.update(event))
            }
            (Multiplex::B(protocol), MultiplexEvent::B(event)) => {
                MultiplexEffect::B(protocol.update(event))
            }
            _ => unreachable!(),
        }
    }
}

pub struct Feedback<A, B>(A, B);

pub enum FeedbackEffect<B, A> {
    Internal(B),
    External(A),
}

impl<A, B, EventA, EventB> Protocol<MultiplexEvent<EventA, EventB>> for Feedback<A, B>
where
    A: Protocol<EventA, Effect = EventB>,
    B: Protocol<EventB, Effect = EventA>,
    A::Effect: Into<FeedbackEffect<EventB, A::Effect>>,
    B::Effect: Into<FeedbackEffect<EventA, B::Effect>>,
{
    type Effect = MultiplexEffect<A::Effect, B::Effect>;

    fn update(&mut self, event: MultiplexEvent<EventA, EventB>) -> Self::Effect {
        match event {
            MultiplexEvent::A(event) => match self.0.update(event).into() {
                FeedbackEffect::Internal(effect) => self.update(MultiplexEvent::B(effect)),
                FeedbackEffect::External(effect) => MultiplexEffect::A(effect),
            },
            MultiplexEvent::B(event) => match self.1.update(event).into() {
                FeedbackEffect::Internal(effect) => self.update(MultiplexEvent::A(effect)),
                FeedbackEffect::External(effect) => MultiplexEffect::B(effect),
            },
        }
    }
}

pub struct Batch<A>(A);

impl<A, E, IterE> Protocol<IterE> for Batch<A>
where
    A: Protocol<E>,
    IterE: Iterator<Item = E>,
{
    type Effect = Box<[A::Effect]>;

    fn update(&mut self, event: IterE) -> Self::Effect {
        event.map(|event| self.0.update(event)).collect()
    }
}

pub trait Protocol<Event> {
    type Effect;
    fn update(&mut self, event: Event) -> Self::Effect;

    fn multiplex<P, E>(self, other: P) -> Multiplex<Self, P>
    where
        Self: Sized,
        P: Protocol<E>,
    {
        Multiplex(self, other)
    }

    fn feedback<P, E>(self, other: P) -> Feedback<Self, P>
    where
        Self: Sized,
        P: Protocol<E>,
    {
        Feedback(self, other)
    }
}

pub struct Multiplex<A, B>(A, B);

pub enum BiEvent<A, B> {
    A(A),
    B(B),
}

pub enum BiEffect<A, B> {
    A(A),
    B(B),
}

impl<A, B, EventA, EventB> Protocol<BiEvent<EventA, EventB>> for Multiplex<A, B>
where
    A: Protocol<EventA>,
    B: Protocol<EventB>,
{
    type Effect = BiEffect<A::Effect, B::Effect>;

    fn update(&mut self, event: BiEvent<EventA, EventB>) -> Self::Effect {
        match event {
            BiEvent::A(event) => BiEffect::A(self.0.update(event)),
            BiEvent::B(event) => BiEffect::B(self.1.update(event)),
        }
    }
}

pub struct Feedback<A, B>(A, B);

pub enum FeedbackEffect<A> {
    Internal(A),
    External(A),
}

impl<A, B, EventA, EventB> Protocol<BiEvent<EventA, EventB>> for Feedback<A, B>
where
    A: Protocol<EventA, Effect = EventB>,
    B: Protocol<EventB, Effect = EventA>,
    A::Effect: Into<FeedbackEffect<A::Effect>>,
    B::Effect: Into<FeedbackEffect<B::Effect>>,
{
    type Effect = BiEffect<A::Effect, B::Effect>;

    fn update(&mut self, event: BiEvent<EventA, EventB>) -> Self::Effect {
        match event {
            BiEvent::A(event) => match self.0.update(event).into() {
                FeedbackEffect::Internal(effect) => self.update(BiEvent::B(effect)),
                FeedbackEffect::External(effect) => BiEffect::A(effect),
            },
            BiEvent::B(event) => match self.1.update(event).into() {
                FeedbackEffect::Internal(effect) => self.update(BiEvent::A(effect)),
                FeedbackEffect::External(effect) => BiEffect::B(effect),
            },
        }
    }
}

use crate::Protocol;

pub struct With<A, B>(A, B);

pub enum WithEvent<A, B> {
    A(A),
    B(B),
}

pub enum WithEffect<A, B> {
    A(A),
    B(B),
}

impl<A, B, EventA, EventB> Protocol<WithEvent<EventA, EventB>> for With<A, B>
where
    A: Protocol<EventA>,
    B: Protocol<EventB>,
{
    type Effect = WithEffect<A::Effect, B::Effect>;

    fn update(&mut self, event: WithEvent<EventA, EventB>) -> Self::Effect {
        match event {
            WithEvent::A(event) => WithEffect::A(self.0.update(event)),
            WithEvent::B(event) => WithEffect::B(self.1.update(event)),
        }
    }
}

pub struct Feedback<A, B>(A, B);

pub enum FeedbackEffect<A> {
    Internal(A),
    External(A),
}

impl<A, B, EventA> Protocol<EventA> for Feedback<A, B>
where
    A: Protocol<EventA>,
    A::Effect: Into<FeedbackEffect<A::Effect>>,
    B: Protocol<A::Effect, Effect = EventA>,
{
    type Effect = A::Effect;
    fn update(&mut self, mut event: EventA) -> Self::Effect {
        loop {
            match self.0.update(event).into() {
                FeedbackEffect::Internal(effect) => event = self.1.update(effect),
                FeedbackEffect::External(effect) => return effect,
            }
        }
    }
}

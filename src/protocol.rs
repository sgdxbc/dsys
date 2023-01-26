pub trait Protocol<Event> {
    type Effect;

    fn init(&mut self) -> Self::Effect;
    fn update(&mut self, event: Event) -> Self::Effect;
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

    fn init(&mut self) -> Self::Effect {
        match self {
            Multiplex::A(protocol) => protocol.init(),
            Multiplex::B(protocol) => protocol.init(),
        }
    }

    fn update(&mut self, event: E) -> Self::Effect {
        match self {
            Multiplex::A(protocol) => protocol.update(event),
            Multiplex::B(protocol) => protocol.update(event),
        }
    }
}

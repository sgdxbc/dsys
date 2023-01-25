pub mod protocol;

pub trait Protocol<Event> {
    type Effect;
    fn update(&mut self, event: Event) -> Self::Effect;
}

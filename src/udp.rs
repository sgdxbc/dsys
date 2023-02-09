use std::{
    borrow::Cow,
    marker::PhantomData,
    net::{SocketAddr, UdpSocket},
    os::fd::AsRawFd,
    panic::panic_any,
    sync::Arc,
};

use bincode::Options;

use nix::{
    errno::Errno,
    poll::{ppoll, PollFd, PollFlags},
    sys::signal::{SigSet, Signal::SIGINT},
};
use serde::{de::DeserializeOwned, Serialize};

use crate::{protocol::Generate, NodeAddr, NodeEffect, Protocol};

pub fn init_socket(socket: &UdpSocket) {
    socket.set_nonblocking(true).unwrap();
    ppoll(
        &mut [PollFd::new(
            socket.as_raw_fd(),
            PollFlags::from_iter([PollFlags::POLLOUT].into_iter()),
        )],
        None,
        None,
    )
    .unwrap();
}

pub enum RxEvent<'a> {
    Receive(Cow<'a, [u8]>),
}

pub struct RxEventOwned(pub RxEvent<'static>);

impl From<RxEvent<'_>> for RxEventOwned {
    fn from(event: RxEvent<'_>) -> Self {
        let event = match event {
            RxEvent::Receive(buf) => RxEvent::Receive(buf.into_owned().into()),
        };
        Self(event)
    }
}

impl From<RxEventOwned> for RxEvent<'static> {
    fn from(event: RxEventOwned) -> Self {
        event.0
    }
}

pub struct NodeRx<M>(PhantomData<M>);

impl<M> Default for NodeRx<M> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<M> Protocol<RxEvent<'_>> for NodeRx<M>
where
    M: DeserializeOwned,
{
    type Effect = M;

    fn update(&mut self, event: RxEvent) -> Self::Effect {
        let RxEvent::Receive(buf) = event;
        bincode::options()
            .allow_trailing_bytes()
            .deserialize(&buf)
            .unwrap()
    }
}

pub struct Rx(pub Arc<UdpSocket>);

impl Generate for Rx {
    type Event<'a> = RxEvent<'a>;

    fn deploy<P>(&mut self, protocol: &mut P)
    where
        P: for<'a> Protocol<Self::Event<'a>>,
    {
        let mut buf = [0; 1500];
        let poll_fd = PollFd::new(self.0.as_raw_fd(), PollFlags::POLLIN);
        let sigmask = SigSet::from_iter([SIGINT].into_iter());
        // can have exit condition
        loop {
            match ppoll(&mut [poll_fd], None, Some(sigmask)) {
                Err(Errno::EINTR) => break,
                Err(err) => panic_any(err),
                Ok(_) => {
                    while let Ok((len, _)) = self.0.recv_from(&mut buf) {
                        protocol.update(RxEvent::Receive(Cow::Borrowed(&buf[..len])));
                    }
                }
            }
        }
    }
}

pub enum TxEvent {
    Nop,
    Send(SocketAddr, Box<[u8]>),
    Broadcast(Box<[u8]>),
}

pub struct NodeTx<M>(PhantomData<M>);

impl<M> Default for NodeTx<M> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<M> Protocol<NodeEffect<M>> for NodeTx<M>
where
    M: Serialize,
{
    type Effect = TxEvent;

    fn update(&mut self, event: NodeEffect<M>) -> Self::Effect {
        match event {
            NodeEffect::Nop => TxEvent::Nop,
            NodeEffect::Send(NodeAddr::Socket(addr), message) => {
                let buf = bincode::options().serialize(&message).unwrap().into();
                TxEvent::Send(addr, buf)
            }
            NodeEffect::Broadcast(message) => {
                let buf = bincode::options().serialize(&message).unwrap().into();
                TxEvent::Broadcast(buf)
            }
            _ => unreachable!(),
        }
    }
}

pub struct Tx {
    socket: Arc<UdpSocket>,
    broadcast: Box<[SocketAddr]>,
}

impl Tx {
    pub fn new(socket: Arc<UdpSocket>) -> Self {
        Self {
            socket,
            broadcast: Default::default(),
        }
    }

    //
}

impl Protocol<TxEvent> for Tx {
    type Effect = ();

    fn update(&mut self, event: TxEvent) -> Self::Effect {
        match event {
            TxEvent::Nop => {}
            TxEvent::Send(addr, buf) => {
                self.socket.send_to(&buf, addr).unwrap();
            }
            TxEvent::Broadcast(buf) => {
                for &addr in &*self.broadcast {
                    self.socket.send_to(&buf, addr).unwrap();
                }
            }
        }
    }
}

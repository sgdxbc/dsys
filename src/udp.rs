use std::{
    borrow::Cow,
    marker::PhantomData,
    net::{SocketAddr, ToSocketAddrs, UdpSocket},
    os::fd::AsRawFd,
    panic::panic_any,
    sync::Arc,
};

use bincode::Options;

use nix::{
    errno::Errno,
    poll::{ppoll, PollFd, PollFlags},
    sys::signal::SigSet,
};
use serde::de::DeserializeOwned;

use crate::{protocol::Generate, NodeAddr, NodeEffect, NodeEvent, Protocol};

// really seek for a better way
pub fn client_socket(remote: impl ToSocketAddrs) -> UdpSocket {
    let socket = UdpSocket::bind("0.0.0.0:0").unwrap();
    socket.connect(remote).unwrap();
    let addr = socket.local_addr().unwrap();
    drop(socket);
    UdpSocket::bind(addr).unwrap()
}

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

pub struct Rx(pub Arc<UdpSocket>);

impl Generate for Rx {
    type Event<'a> = RxEvent<'a>;

    fn deploy<P>(&mut self, protocol: &mut P)
    where
        P: for<'a> Protocol<Self::Event<'a>>,
    {
        let mut buf = [0; 65507];
        loop {
            match ppoll(
                &mut [PollFd::new(self.0.as_raw_fd(), PollFlags::POLLIN)],
                None,
                Some(SigSet::empty()),
            ) {
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
    Send(SocketAddr, Box<[u8]>),
    Broadcast(Box<[u8]>),
}

pub struct Tx {
    socket: Arc<UdpSocket>,
    broadcast: Box<[SocketAddr]>,
}

impl Tx {
    pub fn new(socket: Arc<UdpSocket>, broadcast: Box<[SocketAddr]>) -> Self {
        Self { socket, broadcast }
    }
}

impl Protocol<TxEvent> for Tx {
    type Effect = ();

    fn update(&mut self, event: TxEvent) -> Self::Effect {
        match event {
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

// these are not generic (de)serialization infrastration because they interact with `Rx/TxEvent`
// if find a good way to generalize against those, they can be moved to `crate::node`

pub struct Deserialize<M>(PhantomData<M>);

impl<M> Default for Deserialize<M> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<M> Protocol<RxEvent<'_>> for Deserialize<M>
where
    M: DeserializeOwned,
{
    type Effect = NodeEvent<M>;

    fn update(&mut self, event: RxEvent) -> Self::Effect {
        let RxEvent::Receive(buf) = event;
        NodeEvent::Handle(
            bincode::options()
                .allow_trailing_bytes()
                .deserialize(&buf)
                .unwrap(),
        )
    }
}

pub struct Serialize<M>(PhantomData<M>);

impl<M> Default for Serialize<M> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<M> Protocol<NodeEffect<M>> for Serialize<M>
where
    M: serde::Serialize,
{
    type Effect = TxEvent;

    fn update(&mut self, event: NodeEffect<M>) -> Self::Effect {
        match event {
            NodeEffect::Send(NodeAddr::Socket(addr), message) => {
                let buf = bincode::options().serialize(&message).unwrap().into();
                TxEvent::Send(addr, buf)
            }
            NodeEffect::Send(..) => panic!(),
            NodeEffect::Broadcast(message) => {
                let buf = bincode::options().serialize(&message).unwrap().into();
                TxEvent::Broadcast(buf)
            }
        }
    }
}

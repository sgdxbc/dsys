use std::{
    io::ErrorKind,
    marker::PhantomData,
    net::{SocketAddr, UdpSocket},
    ops::Range,
    os::fd::AsRawFd,
    panic::panic_any,
    sync::Arc,
    thread::{spawn, JoinHandle},
    time::{Duration, Instant},
};

use bincode::Options;
use crossbeam::channel::{self, Receiver, RecvTimeoutError, Sender};

use nix::{
    errno::Errno,
    poll::{ppoll, PollFd, PollFlags},
    sched::{sched_setaffinity, CpuSet},
    sys::signal::{SigSet, Signal::SIGINT},
    unistd::Pid,
};
use serde::{de::DeserializeOwned, Serialize};

use crate::{set_affinity, NodeAddr, NodeEffect, NodeEvent, Protocol};

pub struct TransportConfig {
    pub socket: UdpSocket,
    pub affinity: Option<Range<usize>>,
    pub broadcast: Box<[SocketAddr]>,
}

pub struct TransportRun<N, M> {
    pub receive_thread: JoinHandle<()>,
    node_thread: JoinHandle<N>,
    effect_threads: Vec<JoinHandle<()>>,
    channel: Sender<RunEvent<M>>,
}

enum RunEvent<M> {
    Receive(M),
    Stop,
}

enum EffectEvent<M> {
    Send(NodeAddr, M),
    Broadcast(M),
}

pub fn run<N, M>(node: N, config: TransportConfig) -> TransportRun<N, M>
where
    N: Send + 'static + Protocol<NodeEvent<M>, Effect = NodeEffect<M>>,
    M: Send + 'static + Serialize + DeserializeOwned,
{
    if let Some(affinity) = config.affinity.clone() {
        assert!(affinity.len() >= 3);
    }

    let socket = Arc::new(config.socket);
    let channel = channel::unbounded();
    let effect_channel = channel::unbounded();

    fn set_affinity(affinity: usize) {
        let mut cpu_set = CpuSet::new();
        cpu_set.set(affinity).unwrap();
        sched_setaffinity(Pid::from_raw(0), &cpu_set).unwrap();
    }

    let node_thread = spawn({
        let affinity = config.affinity.clone();
        move || {
            if let Some(affinity) = affinity {
                set_affinity(affinity.start);
            }
            run_node(node, channel.1, effect_channel.0)
        }
    });
    let receive_thread = spawn({
        let affinity = config.affinity.clone();
        let channel = channel.0.clone();
        let socket = socket.clone();
        move || {
            if let Some(affinity) = affinity {
                set_affinity(affinity.start + 1);
            }
            run_receive(channel, socket)
        }
    });
    let mut effect_threads = Vec::new();
    if let Some(affinity) = config.affinity {
        for i in affinity.start + 2..affinity.end {
            let effect_channel = effect_channel.1.clone();
            let socket = socket.clone();
            let broadcast = config.broadcast.clone();
            effect_threads.push(spawn(move || {
                set_affinity(i);
                run_effect(effect_channel, socket, broadcast)
            }))
        }
    } else {
        effect_threads.push(spawn(move || {
            run_effect(effect_channel.1, socket, config.broadcast)
        }))
    }

    TransportRun {
        receive_thread,
        node_thread,
        effect_threads,
        channel: channel.0,
    }
}

impl<N, M> TransportRun<N, M> {
    pub fn stop(self) -> N {
        self.channel.send(RunEvent::Stop).unwrap();
        let node = self.node_thread.join().unwrap();
        for thread in self.effect_threads {
            thread.join().unwrap()
        }
        node
    }
}

fn run_node<N, M>(
    mut node: N,
    channel: Receiver<RunEvent<M>>,
    effect_channel: Sender<EffectEvent<M>>,
) -> N
where
    N: Protocol<NodeEvent<M>, Effect = NodeEffect<M>>,
    M: Send + 'static + Serialize,
{
    perform_effect(node.update(NodeEvent::Init), &effect_channel);
    let mut deadline = Instant::now() + Duration::from_millis(10);
    loop {
        let effect;
        match channel.recv_deadline(deadline) {
            Ok(RunEvent::Receive(message)) => effect = node.update(NodeEvent::Handle(message)),
            Err(RecvTimeoutError::Timeout) => {
                effect = node.update(NodeEvent::Tick);
                deadline = Instant::now() + Duration::from_millis(10);
            }
            // currently disconnected should be unreachable
            Ok(RunEvent::Stop) | Err(RecvTimeoutError::Disconnected) => break,
        }
        perform_effect(effect, &effect_channel);
    }

    fn perform_effect<M>(effect: NodeEffect<M>, channel: &Sender<EffectEvent<M>>) {
        match effect {
            NodeEffect::Compose(effects) => {
                for effect in effects {
                    perform_effect(effect, channel)
                }
            }
            NodeEffect::Nop => {}
            NodeEffect::Send(destination, message) => channel
                .send(EffectEvent::Send(destination, message))
                .unwrap(),
            NodeEffect::Broadcast(message) => {
                channel.send(EffectEvent::Broadcast(message)).unwrap()
            }
        }
    }

    node
}

fn run_receive<M>(channel: Sender<RunEvent<M>>, socket: Arc<UdpSocket>)
where
    M: DeserializeOwned,
{
    let mut buf = [0; 1500];
    loop {
        match socket.recv_from(&mut buf) {
            Ok((len, _)) => {
                let message = bincode::options()
                    .allow_trailing_bytes()
                    .deserialize(&buf[..len])
                    .unwrap();
                // allow send failure because node thread will exit any time
                let _ = channel.send(RunEvent::Receive(message));
            }
            // currently not used, maybe wil use it later
            Err(err) if err.kind() == ErrorKind::Interrupted => break,
            Err(err) => panic_any(err),
        }
    }
}

fn run_effect<M>(
    channel: Receiver<EffectEvent<M>>,
    socket: Arc<UdpSocket>,
    broadcast: Box<[SocketAddr]>,
) where
    M: Serialize,
{
    while let Ok(event) = channel.recv() {
        match event {
            EffectEvent::Send(NodeAddr::Socket(addr), message) => {
                let buf = bincode::options().serialize(&message).unwrap();
                socket.send_to(&buf, addr).unwrap();
            }
            EffectEvent::Send(..) => unreachable!(),
            EffectEvent::Broadcast(message) => {
                let buf = bincode::options().serialize(&message).unwrap();
                for &addr in &*broadcast {
                    socket.send_to(&buf, addr).unwrap();
                }
            }
        }
    }
}

pub enum RxEvent {
    Receive(Box<[u8]>),
}

pub struct NodeRx<M>(PhantomData<M>);

impl<M> Default for NodeRx<M> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<M> Protocol<RxEvent> for NodeRx<M>
where
    M: DeserializeOwned,
{
    type Effect = NodeEvent<M>;

    fn update(&mut self, event: RxEvent) -> Self::Effect {
        let RxEvent::Receive(buf) = event;
        let message = bincode::options()
            .allow_trailing_bytes()
            .deserialize(&buf)
            .unwrap();
        NodeEvent::Handle(message)
    }
}

pub fn spawn_rx<P>(
    socket: Arc<UdpSocket>,
    affinity: Option<usize>,
    mut protocol: P,
    channel: channel::Sender<P::Effect>,
) -> JoinHandle<P>
where
    P: Protocol<RxEvent> + Send + 'static,
    P::Effect: Send + 'static,
{
    socket.set_nonblocking(true).unwrap();

    spawn(move || {
        set_affinity(affinity);

        let mut buf = [0; 1500];
        let sigmask = SigSet::from_iter([SIGINT].into_iter());
        // can have exit condition
        loop {
            match ppoll(
                &mut [PollFd::new(socket.as_raw_fd(), PollFlags::POLLIN)],
                None,
                Some(sigmask),
            ) {
                Err(Errno::EINTR) => break,
                Err(err) => panic_any(err),
                Ok(_) => {
                    while let Ok((len, _)) = socket.recv_from(&mut buf) {
                        let effect = protocol.update(RxEvent::Receive(buf[..len].to_vec().into()));
                        channel.send(effect).unwrap()
                    }
                }
            }
        }

        protocol
    })
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

use std::{
    io::ErrorKind,
    net::UdpSocket,
    panic::panic_any,
    sync::Arc,
    thread::{spawn, JoinHandle},
    time::{Duration, Instant},
};

use bincode::Options;
use crossbeam::channel::{self, Receiver, RecvTimeoutError, Sender};

use serde::{de::DeserializeOwned, Serialize};

use crate::{NodeAddr, NodeEffect, NodeEvent, Protocol};

pub struct TransportRun<N, M> {
    pub receive_thread: JoinHandle<()>,
    node_thread: JoinHandle<N>,
    channel: Sender<RunEvent<M>>,
}

enum RunEvent<M> {
    Receive(M),
    Stop,
}

pub fn run<N, M>(node: N, socket: UdpSocket) -> TransportRun<N, M>
where
    N: Send + 'static + Protocol<NodeEvent<M>, Effect = NodeEffect<M>>,
    M: Send + 'static + Serialize + DeserializeOwned,
{
    let socket = Arc::new(socket);
    let channel = channel::unbounded();
    let node_thread = spawn({
        let socket = socket.clone();
        move || run_node(node, channel.1, socket)
    });
    let receive_thread = spawn({
        let channel = channel.0.clone();
        move || run_receive(channel, socket)
    });

    TransportRun {
        receive_thread,
        node_thread,
        channel: channel.0,
    }
}

impl<N, M> TransportRun<N, M> {
    pub fn stop(self) -> N {
        self.channel.send(RunEvent::Stop).unwrap();
        self.node_thread.join().unwrap()
    }
}

fn run_node<N, M>(mut node: N, channel: Receiver<RunEvent<M>>, socket: Arc<UdpSocket>) -> N
where
    N: Protocol<NodeEvent<M>, Effect = NodeEffect<M>>,
    M: Send + 'static + Serialize,
{
    let effect_channel = channel::unbounded();
    let effect_thread = spawn(move || run_effect(effect_channel.1, socket));

    perform_effect(node.init(), &effect_channel.0);
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
        perform_effect(effect, &effect_channel.0);
    }

    fn perform_effect<M>(effect: NodeEffect<M>, channel: &Sender<(NodeAddr, M)>) {
        match effect {
            NodeEffect::Compose(effects) => {
                for effect in effects {
                    perform_effect(effect, channel)
                }
            }
            NodeEffect::Nop => {}
            NodeEffect::Notify(_) => panic!(),
            NodeEffect::Send(destination, message) => channel.send((destination, message)).unwrap(),
        }
    }

    drop(effect_channel.0);
    effect_thread.join().unwrap();
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

fn run_effect<M>(channel: Receiver<(NodeAddr, M)>, socket: Arc<UdpSocket>)
where
    M: Serialize,
{
    while let Ok((addr, message)) = channel.recv() {
        let NodeAddr::Socket(addr) = addr else {
                panic!()
            };
        let buf = bincode::options().serialize(&message).unwrap();
        socket.send_to(&buf, addr).unwrap();
    }
}

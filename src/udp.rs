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

pub struct Transport<N, M> {
    socket: Arc<UdpSocket>,
    node_thread: JoinHandle<N>,
    channel: Sender<M>,
}

impl<N, M> Transport<N, M> {
    pub fn new(node: N, socket: UdpSocket) -> Self
    where
        N: Send + 'static + Protocol<NodeEvent<M>, Effect = NodeEffect<M>>,
        M: Send + 'static + Serialize,
    {
        let socket = Arc::new(socket);
        let channel = channel::unbounded();
        Self {
            socket: socket.clone(),
            node_thread: spawn(move || Self::run_node(node, channel.1, socket)),
            channel: channel.0,
        }
    }

    fn run_node(mut node: N, channel: Receiver<M>, socket: Arc<UdpSocket>) -> N
    where
        N: Protocol<NodeEvent<M>, Effect = NodeEffect<M>>,
        M: Send + 'static + Serialize,
    {
        let effect_channel = channel::unbounded();
        let effect_thread = spawn(move || Self::run_effect(effect_channel.1, socket));

        perform_effect(node.init(), &effect_channel.0);
        let mut deadline = Instant::now() + Duration::from_millis(10);
        loop {
            let effect;
            match channel.recv_deadline(deadline) {
                Ok(message) => effect = node.update(NodeEvent::Handle(message)),
                Err(RecvTimeoutError::Timeout) => {
                    effect = node.update(NodeEvent::Tick);
                    deadline = Instant::now() + Duration::from_millis(10);
                }
                Err(RecvTimeoutError::Disconnected) => break,
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
                NodeEffect::Send(destination, message) => {
                    channel.send((destination, message)).unwrap()
                }
            }
        }

        drop(effect_channel.0);
        effect_thread.join().unwrap();
        node
    }

    fn run_effect(channel: Receiver<(NodeAddr, M)>, socket: Arc<UdpSocket>)
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

    pub fn run(&mut self)
    where
        M: DeserializeOwned,
    {
        let mut buf = [0; 1500];
        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((len, _)) => {
                    let message = bincode::options()
                        .allow_trailing_bytes()
                        .deserialize(&buf[..len])
                        .unwrap();
                    self.channel.send(message).unwrap();
                }
                Err(err) if err.kind() == ErrorKind::Interrupted => break,
                Err(err) => panic_any(err),
            }
        }
    }

    pub fn stop(self) -> N {
        drop(self.channel);
        self.node_thread.join().unwrap()
    }
}

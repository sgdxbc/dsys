use std::{
    io::ErrorKind,
    net::UdpSocket,
    ops::Range,
    panic::panic_any,
    sync::Arc,
    thread::{spawn, JoinHandle},
    time::{Duration, Instant},
};

use bincode::Options;
use crossbeam::channel::{self, Receiver, RecvTimeoutError, Sender};

use nix::{
    sched::{sched_setaffinity, CpuSet},
    unistd::Pid,
};
use serde::{de::DeserializeOwned, Serialize};

use crate::{NodeAddr, NodeEffect, NodeEvent, Protocol};

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

pub fn run<N, M>(node: N, socket: UdpSocket, affinity: Option<Range<usize>>) -> TransportRun<N, M>
where
    N: Send + 'static + Protocol<NodeEvent<M>, Effect = NodeEffect<M>>,
    M: Send + 'static + Serialize + DeserializeOwned,
{
    if let Some(affinity) = affinity.clone() {
        assert!(affinity.len() >= 3);
    }

    let socket = Arc::new(socket);
    let channel = channel::unbounded();
    let effect_channel = channel::unbounded();

    fn set_affinity(affinity: usize) {
        let mut cpu_set = CpuSet::new();
        cpu_set.set(affinity).unwrap();
        sched_setaffinity(Pid::from_raw(0), &cpu_set).unwrap();
    }

    let node_thread = spawn({
        let affinity = affinity.clone();
        move || {
            if let Some(affinity) = affinity {
                set_affinity(affinity.start);
            }
            run_node(node, channel.1, effect_channel.0)
        }
    });
    let receive_thread = spawn({
        let affinity = affinity.clone();
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
    if let Some(affinity) = affinity {
        for i in affinity.start + 2..affinity.end {
            let effect_channel = effect_channel.1.clone();
            let socket = socket.clone();
            effect_threads.push(spawn(move || {
                set_affinity(i);
                run_effect(effect_channel, socket)
            }))
        }
    } else {
        effect_threads.push(spawn(move || run_effect(effect_channel.1, socket)))
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
    effect_channel: Sender<(NodeAddr, M)>,
) -> N
where
    N: Protocol<NodeEvent<M>, Effect = NodeEffect<M>>,
    M: Send + 'static + Serialize,
{
    perform_effect(node.init(), &effect_channel);
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

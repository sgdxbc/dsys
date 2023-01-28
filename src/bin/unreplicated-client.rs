use std::{
    env::args,
    iter::repeat_with,
    net::{SocketAddr, UdpSocket},
    sync::{
        atomic::{AtomicU32, Ordering},
        mpsc, Arc,
    },
    thread::{sleep, spawn},
    time::Duration,
};

use dsys::{node::Workload, udp::Transport, unreplicated::Client, NodeAddr};
use nix::sys::{
    pthread::{pthread_kill, pthread_self},
    signal::Signal::SIGINT,
};
use rand::random;

fn main() {
    let ip = args().nth(1).unwrap_or(String::from("localhost"));
    let socket = UdpSocket::bind((ip, 0)).unwrap();
    let count = Arc::new(AtomicU32::new(0));
    let node = Workload::new_benchmark(
        Client::new(
            random(),
            NodeAddr::Socket(socket.local_addr().unwrap()),
            NodeAddr::Socket(SocketAddr::from(([172, 31, 1, 1], 5000))),
        ),
        repeat_with::<Box<[u8]>, _>(Default::default),
        count.clone(),
    );
    let pthread_channel = mpsc::sync_channel(1);
    let transport_thread = spawn(move || {
        pthread_channel.0.send(pthread_self()).unwrap();
        let mut transport = Transport::new(node, socket);
        transport.run();
        transport.stop()
    });
    for _ in 0..10 {
        sleep(Duration::from_secs(1));
        println!("{}", count.swap(0, Ordering::SeqCst));
    }
    pthread_kill(pthread_channel.1.recv().unwrap(), SIGINT).unwrap();
    let node = transport_thread.join().unwrap();
    let mut latencies = node.latencies;
    if !latencies.is_empty() {
        latencies.sort_unstable();
        println!(
            "50th {:?} 99th {:?}",
            latencies[latencies.len() / 2],
            latencies[latencies.len() * 99 / 100]
        );
    }
}

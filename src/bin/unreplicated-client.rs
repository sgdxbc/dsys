use std::{
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

fn main() {
    let socket = UdpSocket::bind(("0.0.0.0", 0)).unwrap();
    let count = Arc::new(AtomicU32::new(0));
    let node = Workload::new_benchmark(
        Client::new(
            0,
            NodeAddr::Socket(socket.local_addr().unwrap()),
            NodeAddr::Socket(SocketAddr::from(([127, 0, 0, 1], 5000))),
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
    sleep(Duration::from_secs(1));
    println!("{}", count.swap(0, Ordering::SeqCst));
    pthread_kill(pthread_channel.1.recv().unwrap(), SIGINT).unwrap();
    transport_thread.join().unwrap();
}

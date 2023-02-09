use std::{
    env::args,
    net::UdpSocket,
    sync::Arc,
    thread::{available_parallelism, spawn},
};

use crossbeam::channel;
use dsys::{
    app,
    node::Lifecycle,
    protocol::{Generate, Identity},
    udp,
    unreplicated::Replica,
    App, Protocol,
};
use nix::{
    sched::{sched_setaffinity, CpuSet},
    unistd::Pid,
};

fn set_affinity(affinity: usize) {
    let mut cpu_set = CpuSet::new();
    cpu_set.set(affinity).unwrap();
    sched_setaffinity(Pid::from_raw(0), &cpu_set).unwrap();
}

fn main() {
    let ip = args().nth(1).unwrap_or(String::from("localhost"));
    let socket = Arc::new(UdpSocket::bind((ip, 5000)).unwrap());
    udp::init_socket(&socket);
    let node = Replica::new(App::Null(app::Null));

    let event_channel = channel::unbounded();
    let mut rx = udp::Rx(socket.clone());
    let rx = spawn(move || {
        set_affinity(0);
        rx.deploy(&mut udp::NodeRx::default().then(event_channel.0))
    });

    let effect_channel = channel::unbounded();
    let _node = spawn(move || {
        set_affinity(1);
        Lifecycle::new(event_channel.1, Default::default()).deploy(&mut node.then(effect_channel.0))
    });

    for i in 2..available_parallelism().unwrap().get() - 1 {
        let mut effect_channel = effect_channel.1.clone();
        let socket = socket.clone();
        let _tx = spawn(move || {
            set_affinity(i);
            effect_channel
                .deploy(&mut Identity.each_then(udp::NodeTx::default().then(udp::Tx::new(socket))))
        });
    }

    rx.join().unwrap();
}

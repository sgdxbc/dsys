use std::{env::args, iter::repeat_with, net::UdpSocket, sync::Arc, thread::available_parallelism};

use crossbeam::channel;
use dsys::{app, node, udp, unreplicated::Replica, App, Protocol};

fn main() {
    let ip = args().nth(1).unwrap_or(String::from("localhost"));
    let socket = Arc::new(UdpSocket::bind((ip, 5000)).unwrap());
    let node = Replica::new(App::Null(app::Null));

    let event_channel = channel::unbounded();
    let rx = udp::spawn_rx(
        socket.clone(),
        Some(0),
        udp::NodeRx::default(),
        event_channel.0,
    );
    let effect_channel = channel::unbounded();
    let _node = node::spawn(node, Some(1), event_channel.1, effect_channel.0);
    dsys::protocol::spawn(
        repeat_with(|| udp::NodeTx::default().then(udp::Tx::new(socket.clone())))
            // 1 for rx, 1 for node, 1 for IRQ handler
            .take(available_parallelism().unwrap().get() - 3)
            .collect(),
        Some(2),
        effect_channel.1,
        None,
    );
    rx.join().unwrap();
}

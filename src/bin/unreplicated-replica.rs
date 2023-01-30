use std::{env::args, net::UdpSocket, thread::available_parallelism};

use dsys::{
    app,
    udp::{run, TransportConfig},
    unreplicated::Replica,
    App,
};

fn main() {
    let ip = args().nth(1).unwrap_or(String::from("localhost"));
    let node = Replica::new(App::Null(app::Null));
    let transport = run(
        node,
        TransportConfig {
            socket: UdpSocket::bind((ip, 5000)).unwrap(),
            affinity: Some(0..(available_parallelism().unwrap().get() - 1)), // reserve one for external use
            broadcast: Default::default(),
        },
    );
    transport.receive_thread.join().unwrap() // kill manually
}

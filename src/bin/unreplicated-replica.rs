use std::{env::args, net::UdpSocket, thread::available_parallelism};

use dsys::{app, udp::run, unreplicated::Replica, App};

fn main() {
    let ip = args().nth(1).unwrap_or(String::from("localhost"));
    let node = Replica::new(App::Null(app::Null));
    let transport = run(
        node,
        UdpSocket::bind((ip, 5000)).unwrap(),
        Some(0..(available_parallelism().unwrap().get() - 1)), // reserve one for external use
    );
    transport.receive_thread.join().unwrap() // kill manually
}

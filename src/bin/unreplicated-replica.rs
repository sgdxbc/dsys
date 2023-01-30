use std::{env::args, net::UdpSocket};

use dsys::{app, udp::run, unreplicated::Replica, App};

fn main() {
    let ip = args().nth(1).unwrap_or(String::from("localhost"));
    let node = Replica::new(App::Echo(app::Echo));
    let transport = run(node, UdpSocket::bind((ip, 5000)).unwrap());
    transport.receive_thread.join().unwrap() // kill manually
}

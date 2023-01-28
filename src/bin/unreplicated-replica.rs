use std::{env::args, net::UdpSocket};

use dsys::{app, udp::Transport, unreplicated::Replica, App};

fn main() {
    let ip = args().nth(1).unwrap_or(String::from("localhost"));
    let node = Replica::new(App::Echo(app::Echo));
    let mut transport = Transport::new(node, UdpSocket::bind((ip, 5000)).unwrap());
    transport.run();
    println!();
    transport.stop(); // inspect `node` if needed
}

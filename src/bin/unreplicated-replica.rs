use std::net::UdpSocket;

use dsys::{app, udp::Transport, unreplicated::Replica, App};

fn main() {
    let node = Replica::new(App::Echo(app::Echo));
    let mut transport = Transport::new(node, UdpSocket::bind(("0.0.0.0", 5000)).unwrap());
    transport.run();
    println!();
    transport.stop(); // inspect `node` if needed
}

use std::{
    env::args,
    net::{SocketAddr, UdpSocket},
    sync::Arc,
    thread::spawn,
};

use crossbeam::channel;
use dsys::{protocol::Generate, set_affinity, udp, Protocol};

fn main() {
    udp::capture_interrupt();
    let socket = Arc::new(UdpSocket::bind(("0.0.0.0", 5000)).unwrap());
    neo::init_socket(&socket, Some([239, 255, 1, 1].into()));
    let tx = SocketAddr::new(args().nth(1).unwrap().parse().unwrap(), 5000);

    let mut channel = channel::unbounded();
    let rx = spawn({
        set_affinity(0);
        let socket = socket.clone();
        move || {
            udp::Rx(socket).deploy(
                &mut (|udp::RxEvent::Receive(buf): udp::RxEvent<'_>| {
                    udp::TxEvent::Send(tx, buf.into())
                })
                .then(channel.0),
            );
        }
    });

    let _tx = spawn(move || {
        set_affinity(1);
        channel.1.deploy(&mut udp::Tx::new(socket))
    });

    rx.join().unwrap();
}

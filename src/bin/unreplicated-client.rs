use std::{
    ffi::c_int,
    iter::repeat_with,
    net::{SocketAddr, UdpSocket},
    sync::mpsc,
    thread::{sleep, spawn},
    time::Duration,
};

use dsys::{node::Workload, udp::Transport, unreplicated::Client, NodeAddr};
use nix::sys::{
    pthread::{pthread_kill, pthread_self},
    signal::{sigaction, SaFlags, SigAction, SigHandler::Handler, Signal::SIGINT},
    signalfd::SigSet,
};

fn main() {
    let socket = UdpSocket::bind(("0.0.0.0", 0)).unwrap();
    let node = Workload::new(
        Client::new(
            0,
            NodeAddr::Socket(socket.local_addr().unwrap()),
            NodeAddr::Socket(SocketAddr::from(([127, 0, 0, 1], 5000))),
        ),
        repeat_with::<Box<[u8]>, _>(Default::default).take(100),
    );
    let pthread_channel = mpsc::sync_channel(1);
    let transport_thread = spawn(move || {
        extern "C" fn nop(_: c_int) {}
        let action = &SigAction::new(Handler(nop), SaFlags::empty(), SigSet::empty());
        unsafe { sigaction(SIGINT, action) }.unwrap();
        pthread_channel.0.send(pthread_self()).unwrap();

        let mut transport = Transport::new(node, socket);
        transport.run();
        transport.stop()
    });
    sleep(Duration::from_secs(1));
    pthread_kill(pthread_channel.1.recv().unwrap(), SIGINT).unwrap();
    transport_thread.join().unwrap();
}

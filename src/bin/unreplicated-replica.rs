use std::{ffi::c_int, net::UdpSocket};

use dsys::{app, udp::Transport, unreplicated::Replica, App};
use nix::sys::{
    signal::{sigaction, SaFlags, SigAction, SigHandler::Handler, Signal::SIGINT},
    signalfd::SigSet,
};

fn main() {
    extern "C" fn nop(_: c_int) {}
    let action = &SigAction::new(Handler(nop), SaFlags::empty(), SigSet::empty());
    unsafe { sigaction(SIGINT, action) }.unwrap();

    let node = Replica::new(App::Echo(app::Echo));
    let mut transport = Transport::new(node, UdpSocket::bind(("0.0.0.0", 5000)).unwrap());
    transport.run();
    println!();
    transport.stop(); // inspect `node` if needed
}

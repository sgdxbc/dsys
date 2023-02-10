use std::{
    net::{Ipv4Addr, UdpSocket},
    sync::Arc,
    thread::{available_parallelism, spawn},
};

use clap::Parser;
use crossbeam::channel;
use dsys::{protocol::Generate, set_affinity, udp, Protocol};
use neo::Sequencer;

#[derive(Debug, Parser)]
struct Cli {
    #[clap(long)]
    multicast: Ipv4Addr,
    #[clap(long)]
    replica_count: u32,
}

fn main() {
    let cli = Cli::parse();
    let socket = Arc::new(UdpSocket::bind("0.0.0.0:5001").unwrap());
    neo::init_socket(&socket, None); // only send multicast

    // core 0: udp::Rx -> Sequencer -> _chan_
    let mut rx = udp::Rx(socket.clone());
    let channel = channel::unbounded();
    let seq = spawn(move || {
        set_affinity(0);
        rx.deploy(&mut Sequencer::default().then(channel.0))
    });
    // core 1..: _chan_ ~> SipHash -> udp::Tx
    for i in 1..available_parallelism().unwrap().get() - 1 {
        let mut tx = neo::seq::SipHash {
            channel: channel.1.clone(),
            multicast_addr: (cli.multicast, 5000).into(),
            replica_count: cli.replica_count,
        };
        let socket = socket.clone();
        let _tx = spawn(move || {
            set_affinity(i);
            tx.deploy(&mut udp::Tx::new(socket))
        });
    }

    seq.join().unwrap()
}

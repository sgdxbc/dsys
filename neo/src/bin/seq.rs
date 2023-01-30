use std::{
    net::{IpAddr, SocketAddr, UdpSocket},
    thread::available_parallelism,
};

use clap::Parser;
use dsys::udp::{run, TransportConfig};
use neo::Sequencer;

#[derive(Debug, Parser)]
struct Cli {
    #[clap(long)]
    addr: Option<IpAddr>,
    #[clap(long)]
    broadcast: Vec<IpAddr>,
}

fn main() {
    let cli = Cli::parse();
    let transport = run(
        Sequencer::default(),
        TransportConfig {
            socket: UdpSocket::bind((cli.addr.unwrap_or("127.0.0.1".parse().unwrap()), 5001))
                .unwrap(),
            affinity: Some(0..(available_parallelism().unwrap().get() - 1)),
            broadcast: cli
                .broadcast
                .into_iter()
                .map(|ip| SocketAddr::from((ip, 5000)))
                .collect(),
        },
    );
    transport.receive_thread.join().unwrap()
}

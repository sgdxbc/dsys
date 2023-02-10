use std::{
    net::{Ipv4Addr, UdpSocket},
    sync::Arc,
    thread::{available_parallelism, spawn},
};

use clap::Parser;
use crossbeam::channel;
use dsys::{protocol::Generate, set_affinity, udp, Protocol};
use neo::{seq, Sequencer};
use secp256k1::SecretKey;

#[derive(Debug, Parser)]
struct Cli {
    #[clap(long)]
    multicast: Ipv4Addr,
    #[clap(long)]
    replica_count: u32,
    #[clap(long)]
    crypto: String,
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
        let multicast_addr = (cli.multicast, 5000).into();
        let mut channel = channel.1.clone();
        let socket = socket.clone();
        let tx = match &*cli.crypto {
            "siphash" => Box::new(move || {
                seq::SipHash {
                    channel,
                    multicast_addr,
                    replica_count: cli.replica_count,
                }
                .deploy(&mut udp::Tx::new(socket))
            }) as Box<dyn FnOnce() + Send>,
            "p256" => Box::new(move || {
                channel.deploy(
                    &mut seq::P256::new(
                        multicast_addr,
                        SecretKey::from_slice(&[b"seq", &[0; 29][..]].concat()).unwrap(),
                    )
                    .then(udp::Tx::new(socket)),
                )
            }),
            _ => panic!(),
        };
        let _tx = spawn(move || {
            set_affinity(i);
            tx()
        });
    }

    seq.join().unwrap()
}

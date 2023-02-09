use std::{
    convert::identity,
    net::{Ipv4Addr, UdpSocket},
    sync::Arc,
    thread::{available_parallelism, spawn},
};

use clap::Parser;
use crossbeam::channel;
use dsys::{
    app,
    node::Lifecycle,
    protocol::{Generate, Map},
    set_affinity, udp, App, Protocol,
};
use neo::{replica::MulticastCrypto, Replica, RxP256};

#[derive(Debug, Parser)]
struct Cli {
    #[clap(long)]
    multicast: Ipv4Addr,
    #[clap(long)]
    id: u8,
    #[clap(long)]
    crypto: String,
    #[clap(short)]
    f: usize,
}

fn main() {
    let cli = Cli::parse();
    let socket = Arc::new(UdpSocket::bind(("0.0.0.0", 5000)).unwrap());
    neo::init_socket(&socket, Some(cli.multicast));
    let multicast_crypto = match &*cli.crypto {
        "siphash" => MulticastCrypto::SipHash,
        "p256" => MulticastCrypto::P256,
        _ => panic!(),
    };
    let node = Replica::new(cli.id, App::Null(app::Null), cli.f, multicast_crypto);
    let rx = match &*cli.crypto {
        "siphash" => neo::Rx::SipHash { id: cli.id },
        "p256" => neo::Rx::P256,
        _ => panic!(),
    };

    // core 0: udp::Rx -> `rx` -> (_msg_, _p256_)
    let message_channel = channel::unbounded();
    let p256_channel = channel::unbounded();
    let _rx = spawn({
        let message_channel = message_channel.0.clone();
        let socket = socket.clone();
        move || {
            set_affinity(0);
            udp::Rx(socket).deploy(
                &mut rx
                    .then((message_channel, p256_channel.0))
                    .then(Map(Into::into)),
            )
        }
    });
    // core 1: _msg_ ~> Lifecycle -> `node` -> _eff_
    let effect_channel = channel::unbounded();
    let node = spawn(move || {
        set_affinity(1);
        Lifecycle::new(message_channel.1, Default::default())
            .deploy(&mut node.then(effect_channel.0))
    });
    // core 2..: (_eff_, _p256_) -> (neo::Tx -> udp::Tx, RxP256 -> _msg_)
    for i in 2..available_parallelism().unwrap().get() - 1 {
        let socket = socket.clone();
        let effect_channel = effect_channel.1.clone();
        let p256_channel = p256_channel.1.clone();
        let message_channel = message_channel.0.clone();
        let _work = spawn(move || {
            set_affinity(i);
            (effect_channel, p256_channel).deploy(
                &mut (
                    Map(identity).each_then(neo::Tx { multicast: None }.then(udp::Tx::new(socket))),
                    // TODO
                    RxP256::new(None).then(message_channel),
                )
                    .then(Map(Into::into)),
            )
        });
    }

    node.join().unwrap()
}

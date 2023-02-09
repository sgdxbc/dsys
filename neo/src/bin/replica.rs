use std::{
    env::args,
    net::{IpAddr, Ipv4Addr, UdpSocket},
    thread::available_parallelism,
};

use clap::Parser;
use dsys::{app, App};
use neo::Replica;

#[derive(Debug, Parser)]
struct Cli {
    #[clap(long)]
    ip: IpAddr,
    #[clap(long)]
    multicast: Ipv4Addr,
    #[clap(long)]
    id: u8,
}

fn main() {
    let cli = Cli::parse();

    let node = Replica::new(cli.id, App::Null(app::Null));
    //
}

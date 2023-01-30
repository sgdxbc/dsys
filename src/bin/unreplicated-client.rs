use std::{
    env::args,
    fs::File,
    io::Write,
    iter::repeat_with,
    net::{ToSocketAddrs, UdpSocket},
    sync::{
        atomic::{AtomicU8, Ordering},
        Arc,
    },
    thread::sleep,
    time::Duration,
};

use dsys::{
    node::{Workload, WorkloadMode},
    udp::run,
    unreplicated::Client,
    NodeAddr,
};

use rand::random;

fn main() {
    // create the result file first so if crash later file will keep empty
    let mut result = File::create("result.txt").unwrap();

    let ip = args().nth(1).unwrap_or(String::from("localhost"));
    let replica_ip = args().nth(2).unwrap_or(String::from("localhost"));
    let socket = UdpSocket::bind((ip, 0)).unwrap();
    let mode = Arc::new(AtomicU8::new(WorkloadMode::Discard as _));
    let node = Workload::new_benchmark(
        Client::new(
            random(),
            NodeAddr::Socket(socket.local_addr().unwrap()),
            NodeAddr::Socket(
                (replica_ip, 5000)
                    .to_socket_addrs()
                    .unwrap()
                    .next()
                    .unwrap(),
            ),
        ),
        repeat_with::<Box<[u8]>, _>(Default::default),
        mode.clone(),
    );
    let transport = run(node, socket, None);

    sleep(Duration::from_secs(2)); // warm up
    mode.store(WorkloadMode::Benchmark as _, Ordering::SeqCst);
    sleep(Duration::from_secs(10));
    mode.store(WorkloadMode::Discard as _, Ordering::SeqCst);
    sleep(Duration::from_secs(2)); // cool down

    let node = transport.stop();
    let mut latencies = node.latencies;

    writeln!(result, "{}", latencies.len()).unwrap();
    if !latencies.is_empty() {
        latencies.sort_unstable();
        writeln!(
            result,
            "50th {:?} 99th {:?}",
            latencies[latencies.len() / 2],
            latencies[latencies.len() * 99 / 100]
        )
        .unwrap();
    }
}

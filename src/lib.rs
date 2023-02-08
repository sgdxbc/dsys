pub mod app;
pub mod node;
pub mod protocol;
pub mod simulate;
pub mod udp;
pub mod unreplicated;

pub use app::App;
pub use node::{NodeAddr, NodeEffect, NodeEvent};
pub use protocol::Protocol;
pub use simulate::Simulate;

pub fn set_affinity(affinity: Option<usize>) {
    use nix::{
        sched::{sched_setaffinity, CpuSet},
        unistd::Pid,
    };
    if let Some(affinity) = affinity {
        let mut cpu_set = CpuSet::new();
        cpu_set.set(affinity).unwrap();
        sched_setaffinity(Pid::from_raw(0), &cpu_set).unwrap();
    }
}

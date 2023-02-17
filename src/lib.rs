pub mod app;
pub mod crypto;
pub mod node;
pub mod protocol;
pub mod simulate;
pub mod udp;
pub mod unreplicated;

pub use crate::app::App;
pub use crate::node::{NodeAddr, NodeEffect, NodeEvent};
pub use crate::protocol::Protocol;
pub use crate::simulate::Simulate;

// utilities
// if getting too many make them a module

pub fn set_affinity(affinity: usize) {
    use nix::{
        sched::{sched_setaffinity, CpuSet},
        unistd::Pid,
    };
    let mut cpu_set = CpuSet::new();
    cpu_set.set(affinity).unwrap();
    sched_setaffinity(Pid::from_raw(0), &cpu_set).unwrap();
}

pub fn capture_interrupt() {
    use nix::sys::signal::{
        pthread_sigmask, sigaction, SaFlags, SigAction, SigHandler, SigSet, SigmaskHow, Signal,
    };
    use std::ffi::c_int;
    use std::process::abort;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNT: AtomicU32 = AtomicU32::new(0);
    extern "C" fn handle(_: c_int) {
        if COUNT.fetch_add(1, Ordering::SeqCst) == 0 {
            eprintln!("first interruption captured")
        } else {
            abort()
        }
    }
    unsafe {
        sigaction(
            Signal::SIGINT,
            &SigAction::new(
                SigHandler::Handler(handle),
                SaFlags::empty(),
                SigSet::empty(),
            ),
        )
    }
    .unwrap();
    pthread_sigmask(
        SigmaskHow::SIG_BLOCK,
        Some(&SigSet::from_iter([Signal::SIGINT].into_iter())),
        None,
    )
    .unwrap();
}

pub mod client;
pub mod replica;
pub mod seq;

pub use client::Client;
pub use replica::Replica;
pub use seq::Sequencer;

use dsys::NodeAddr;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    client_id: u32,
    client_addr: NodeAddr,
    request_num: u32,
    op: Box<[u8]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Multicast {
    seq: u32,
    signature: [u32; 16],
    digest: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reply {
    request_num: u32,
    result: Box<[u8]>,
    replica_id: u8,
    seq: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Request(Multicast, Request),
    Reply(Reply),
}

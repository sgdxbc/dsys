pub mod client;
pub mod replica;
pub mod seq;

pub use client::Client;
pub use replica::Replica;
pub use seq::Sequencer;

use dsys::{NodeAddr, NodeEffect, NodeEvent};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    client_id: u32,
    client_addr: NodeAddr,
    request_num: u32,
    op: Box<[u8]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[repr(C)]
pub struct Multicast {
    seq: u32,
    signature: [u8; 4],
    digest: [u8; 32],
    payload: Box<[u8]>,
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
    Request(Multicast),
    Reply(Reply),
}

pub type Event = NodeEvent<Message>;
pub type Effect = NodeEffect<Message>;

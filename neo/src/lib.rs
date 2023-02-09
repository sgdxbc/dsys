pub mod client;
pub mod replica;
pub mod seq;

use std::{
    hash::{Hash, Hasher},
    net::{Ipv4Addr, SocketAddr, UdpSocket},
};

pub use crate::client::Client;
pub use crate::replica::Replica;
pub use crate::seq::Sequencer;

use bincode::Options;
use dsys::{
    udp::{RxEvent, TxEvent},
    NodeAddr, NodeEffect, Protocol,
};
use secp256k1::{ecdsa::Signature, PublicKey, Secp256k1, VerifyOnly};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use siphasher::sip::SipHasher;

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
    signature: [[u8; 32]; 2],
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
    Request(Request),
    OrderedRequest(Multicast, Request),
    Reply(Reply),
}

pub fn init_socket(socket: &UdpSocket, multicast_ip: Ipv4Addr) {
    dsys::udp::init_socket(socket);
    socket
        .join_multicast_v4(&multicast_ip, &Ipv4Addr::UNSPECIFIED)
        .unwrap();
}

pub enum RxMulticast {
    SipHash { id: u8 },
    P256 { public_key: PublicKey },
}

pub struct Rx {
    multicast: RxMulticast,
    secp: Secp256k1<VerifyOnly>,
    // replica keys
}

impl Rx {
    pub fn new(multicast: RxMulticast) -> Self {
        Self {
            multicast,
            secp: Secp256k1::verification_only(),
        }
    }
}

// transmission format
// unicast: 4 bytes zero + bincode format `Message`
// multicast on sender:
// + 32 bytes precompute digest
// + 36 bytes zero
// + bincode format `Message`
// multicast on receiver:
// + 4 bytes sequence number
// + 64 bytes signature
// + bincode format `Message`
// (Half)SipHash signature:
// + 4 bytes index of the first MAC
// + 4 bytes MAC1
// + (optional) 4 bytes each MAC2, MAC3, MAC4
// + pad to 64 bytes
// secp256k1 signature: 64 bytes compact format

impl Protocol<RxEvent<'_>> for Rx {
    type Effect = Option<Message>;

    fn update(&mut self, event: RxEvent<'_>) -> Self::Effect {
        let RxEvent::Receive(buf) = event;
        if buf[..4] == [0; 4] {
            // return bincode::options()
            //     .allow_trailing_bytes()
            //     .deserialize(&buf[4..])
            //     .unwrap();
            unreachable!() // for now
        }

        let mut digest = <[u8; 32]>::from(sha2::Sha256::digest(&buf[68..]));
        digest[..4].copy_from_slice(&buf[..4]);
        match &self.multicast {
            RxMulticast::SipHash { id } => {
                let i = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]) as u8;
                if (i..i + 4).contains(id) {
                    let mut hasher = SipHasher::new_with_keys(u64::MAX, *id as _);
                    digest.hash(&mut hasher);
                    let offset = (8 + (id - i)) as usize;
                    if buf[offset..offset + 4] != hasher.finish().to_le_bytes()[..4] {
                        eprintln!("malformed");
                        return None;
                    }
                }
                // otherwise, there is no MAC for this receiver in the packet
            }
            RxMulticast::P256 { public_key } => {
                let message = secp256k1::Message::from_slice(&digest).unwrap();
                if Signature::from_compact(&buf[4..68])
                    .map(|signature| self.secp.verify_ecdsa(&message, &signature, &public_key))
                    .is_err()
                {
                    eprintln!("malformed");
                    return None;
                }
            }
        }

        let multicast = Multicast {
            seq: u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]),
            signature: [
                buf[4..36].try_into().unwrap(),
                buf[36..68].try_into().unwrap(),
            ],
        };
        let Ok(Message::Request(request)) = bincode::options()
            .allow_trailing_bytes()
            .deserialize(&buf[68..])
        else {
            eprintln!("malformed");
            return None;
        };
        Some(Message::OrderedRequest(multicast, request))
    }
}

pub struct Tx {
    // if send to this address, include full 96 bytes multicast header in payload
    multicast: SocketAddr,
}

impl Protocol<NodeEffect<Message>> for Tx {
    type Effect = TxEvent;

    fn update(&mut self, event: NodeEffect<Message>) -> Self::Effect {
        match event {
            NodeEffect::Broadcast(message) => {
                let buf = bincode::options().serialize(&message).unwrap();
                TxEvent::Broadcast([&[0; 4][..], &buf].concat().into())
            }
            NodeEffect::Send(NodeAddr::Socket(addr), message) if addr != self.multicast => {
                let buf = bincode::options().serialize(&message).unwrap();
                TxEvent::Send(addr, [&[0; 4][..], &buf].concat().into())
            }
            NodeEffect::Send(NodeAddr::Socket(_), message) => {
                let buf = bincode::options().serialize(&message).unwrap();
                let digest = <[u8; 32]>::from(sha2::Sha256::digest(&buf));
                TxEvent::Send(
                    self.multicast,
                    [&digest[..], &[0; 36][..], &buf].concat().into(),
                )
            }
            _ => panic!(),
        }
    }
}

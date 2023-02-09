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
    protocol::Multiplex,
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

pub fn init_socket(socket: &UdpSocket, multicast_ip: Option<Ipv4Addr>) {
    dsys::udp::init_socket(socket);
    if let Some(multicast_ip) = multicast_ip {
        assert!(multicast_ip.is_multicast());
        socket
            .join_multicast_v4(&multicast_ip, &Ipv4Addr::UNSPECIFIED)
            .unwrap();
    }
}

pub enum Rx {
    SipHash { id: u8 },
    P256, // key in `RxP256`
    Reject,
}

pub enum RxP256Event {
    Unicast(Box<[u8]>),   // ..4 trimed
    Multicast(Box<[u8]>), // not trimed
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

impl Message {
    fn ordered_request(buf: &[u8]) -> Option<Self> {
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

impl Protocol<RxEvent<'_>> for Rx {
    type Effect = Multiplex<Option<Message>, RxP256Event>;

    fn update(&mut self, event: RxEvent<'_>) -> Self::Effect {
        let RxEvent::Receive(buf) = event;
        if buf[..4] == [0; 4] {
            return Multiplex::B(RxP256Event::Unicast(buf[..4].into()));
        }
        let mut digest = <[u8; 32]>::from(sha2::Sha256::digest(&buf[68..]));
        digest[..4].copy_from_slice(&buf[..4]);
        match self {
            Rx::Reject => Multiplex::A(None),
            Rx::SipHash { id } => {
                let i = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]) as u8;
                if (i..i + 4).contains(id) {
                    let mut hasher = SipHasher::new_with_keys(u64::MAX, *id as _);
                    digest.hash(&mut hasher);
                    let offset = (8 + (*id - i)) as usize;
                    if buf[offset..offset + 4] != hasher.finish().to_le_bytes()[..4] {
                        eprintln!("malformed");
                        return Multiplex::A(None);
                    }
                }
                // otherwise, there is no MAC for this receiver in the packet
                Multiplex::A(Message::ordered_request(&buf))
            }
            Rx::P256 => Multiplex::B(RxP256Event::Multicast(buf.into())),
        }
    }
}

pub struct RxP256 {
    multicast: Option<PublicKey>,
    // replica keys
    secp: Secp256k1<VerifyOnly>,
}

impl RxP256 {
    pub fn new(multicast: Option<PublicKey>) -> Self {
        Self {
            multicast,
            secp: Secp256k1::verification_only(),
        }
    }
}

impl Protocol<RxP256Event> for RxP256 {
    type Effect = Option<Message>;

    fn update(&mut self, event: RxP256Event) -> Self::Effect {
        match event {
            RxP256Event::Unicast(buf) => {
                // TODO verify cross replica messages
                Some(
                    bincode::options()
                        .allow_trailing_bytes()
                        .deserialize(&buf[4..])
                        .unwrap(),
                )
            }
            RxP256Event::Multicast(buf) => {
                let Some(public_key) = &self.multicast else {
                    return None
                };
                let mut digest = <[u8; 32]>::from(sha2::Sha256::digest(&buf[68..]));
                digest[..4].copy_from_slice(&buf[..4]);
                let message = secp256k1::Message::from_slice(&digest).unwrap();
                if Signature::from_compact(&buf[4..68])
                    .map(|signature| self.secp.verify_ecdsa(&message, &signature, public_key))
                    .is_err()
                {
                    eprintln!("malformed");
                    None
                } else {
                    Message::ordered_request(&buf)
                }
            }
        }
    }
}

pub struct Tx {
    // if send to this address, include full 96 bytes multicast header in payload
    pub multicast: Option<SocketAddr>,
}

impl Protocol<NodeEffect<Message>> for Tx {
    type Effect = TxEvent;

    fn update(&mut self, event: NodeEffect<Message>) -> Self::Effect {
        match event {
            NodeEffect::Broadcast(message) => {
                let buf = bincode::options().serialize(&message).unwrap();
                TxEvent::Broadcast([&[0; 4][..], &buf].concat().into())
            }
            NodeEffect::Send(NodeAddr::Socket(addr), message) if Some(addr) != self.multicast => {
                let buf = bincode::options().serialize(&message).unwrap();
                TxEvent::Send(addr, [&[0; 4][..], &buf].concat().into())
            }
            NodeEffect::Send(NodeAddr::Socket(_), message) => {
                let buf = bincode::options().serialize(&message).unwrap();
                let digest = <[u8; 32]>::from(sha2::Sha256::digest(&buf));
                TxEvent::Send(
                    self.multicast.unwrap(),
                    [&digest[..], &[0; 36][..], &buf].concat().into(),
                )
            }
            _ => panic!(),
        }
    }
}

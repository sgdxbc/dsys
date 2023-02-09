use std::{
    hash::{Hash, Hasher},
    net::SocketAddr,
};

use crossbeam::channel;
use dsys::{
    protocol::Generate,
    udp::{RxEvent, TxEvent},
    Protocol,
};
use secp256k1::{Secp256k1, SecretKey, SignOnly};
use siphasher::sip::SipHasher;

// see comment in `lib.rs` for transmission format

#[derive(Default)]
pub struct Sequencer(u32);

impl Protocol<RxEvent<'_>> for Sequencer {
    type Effect = Box<[u8]>;

    fn update(&mut self, event: RxEvent<'_>) -> Self::Effect {
        let RxEvent::Receive(mut buf) = event;
        self.0 += 1;
        buf.to_mut()[..4].copy_from_slice(&self.0.to_be_bytes());
        buf.into()
    }
}

pub struct SipHash {
    pub channel: channel::Receiver<Box<[u8]>>,
    pub multicast_addr: SocketAddr,
    pub replica_count: u32,
}

impl Generate for SipHash {
    type Event<'a> = TxEvent;

    fn deploy<P>(&mut self, protocol: &mut P)
    where
        P: for<'a> Protocol<Self::Event<'a>, Effect = ()>,
    {
        for mut buf in self.channel.iter() {
            let mut i = 0;
            while i < self.replica_count {
                buf[4..8].copy_from_slice(&i.to_ne_bytes());
                for j in i..u32::min(i + 4, self.replica_count) {
                    let mut hasher = SipHasher::new_with_keys(u64::MAX, j as _);
                    buf[..32].hash(&mut hasher);
                    let offset = (8 + (j - i) * 4) as usize;
                    buf[offset..offset + 4].copy_from_slice(&hasher.finish().to_le_bytes()[..4]);
                }
                protocol.update(TxEvent::Send(self.multicast_addr, buf.clone()));
                i += 4;
            }
        }
    }
}

pub struct P256 {
    multicast_addr: SocketAddr,
    secret_key: SecretKey,
    secp: Secp256k1<SignOnly>,
}

impl P256 {
    pub fn new(multicast_addr: SocketAddr, secret_key: SecretKey) -> Self {
        Self {
            multicast_addr,
            secret_key,
            secp: Secp256k1::signing_only(),
        }
    }
}

impl Protocol<Box<[u8]>> for P256 {
    type Effect = TxEvent;

    fn update(&mut self, mut buf: Box<[u8]>) -> Self::Effect {
        let message = secp256k1::Message::from_slice(&buf[..32]).unwrap();
        let signature = self.secp.sign_ecdsa(&message, &self.secret_key);
        buf[4..68].copy_from_slice(&signature.serialize_compact());
        TxEvent::Send(self.multicast_addr, buf)
    }
}

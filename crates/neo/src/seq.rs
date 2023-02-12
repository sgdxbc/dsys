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
use sha2::Digest;
use siphasher::sip::SipHasher;

pub struct Sequencer {
    seq: u32,
    link_hash: [u8; 32],
    pub should_link: Box<dyn FnMut() -> bool + Send>,
}

impl Default for Sequencer {
    fn default() -> Self {
        Self {
            seq: 0,
            link_hash: Default::default(),
            should_link: Box::new(|| false),
        }
    }
}

impl Protocol<RxEvent<'_>> for Sequencer {
    type Effect = (Box<[u8]>, bool);

    fn update(&mut self, event: RxEvent<'_>) -> Self::Effect {
        let RxEvent::Receive(mut buf) = event;
        self.seq += 1;
        buf.to_mut()[..4].copy_from_slice(&self.seq.to_be_bytes());
        buf.to_mut()[32..64].copy_from_slice(&self.link_hash);
        self.link_hash = sha2::Sha256::digest(&buf[..64]).into();
        let should_link = (self.should_link)();
        if should_link {
            buf.to_mut().copy_within(32..64, 4);
            buf.to_mut()[36..68].copy_from_slice(&[0; 32]);
        }
        (buf.into(), should_link)
    }
}

pub struct SipHash {
    pub channel: channel::Receiver<Box<[u8]>>,
    pub multicast_addr: SocketAddr,
    pub replica_count: u8,
}

impl Generate for SipHash {
    type Event<'a> = TxEvent;

    fn deploy<P>(&mut self, protocol: &mut P)
    where
        P: for<'a> Protocol<Self::Event<'a>, Effect = ()>,
    {
        for buf in self.channel.iter() {
            let mut signatures = [0; 16];
            let mut index = 0;
            while index < self.replica_count {
                let mut buf = buf.clone();
                for j in index..u8::min(index + 4, self.replica_count) {
                    let mut hasher = SipHasher::new_with_keys(u64::MAX, j as _);
                    buf[..32].hash(&mut hasher);

                    let offset = (j - index) as usize * 4;
                    signatures[offset..offset + 4]
                        .copy_from_slice(&hasher.finish().to_le_bytes()[..4]);
                    // println!("signature[{j}] {:02x?}", &signatures[offset..offset + 4]);
                }
                buf[4] = index;
                buf[5..21].copy_from_slice(&signatures);

                protocol.update(TxEvent::Send(self.multicast_addr, buf));
                index += 4;
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

impl Protocol<(Box<[u8]>, bool)> for P256 {
    type Effect = TxEvent;

    fn update(&mut self, (mut buf, should_link): (Box<[u8]>, bool)) -> Self::Effect {
        if !should_link {
            let message = secp256k1::Message::from_slice(&buf[..32]).unwrap();
            let signature = self
                .secp
                .sign_ecdsa(&message, &self.secret_key)
                .serialize_compact();

            buf[4..68].copy_from_slice(&signature);
        }
        TxEvent::Send(self.multicast_addr, buf)
    }
}

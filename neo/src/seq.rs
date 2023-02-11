use std::{
    hash::{Hash, Hasher},
    net::SocketAddr,
};

use crossbeam::channel;
use dsys::{
    protocol::Generate,
    udp::{RxEvent, RxEventOwned, TxEvent},
    Protocol,
};
use secp256k1::{Secp256k1, SecretKey, SignOnly};
use siphasher::sip::SipHasher;

#[derive(Default)]
pub struct Sequencer(u32);

impl Protocol<RxEvent<'_>> for Sequencer {
    type Effect = (u32, RxEventOwned);

    fn update(&mut self, event: RxEvent<'_>) -> Self::Effect {
        self.0 += 1;
        (self.0, event.into())
    }
}

pub struct SipHash {
    pub channel: channel::Receiver<(u32, RxEventOwned)>,
    pub multicast_addr: SocketAddr,
    pub replica_count: u8,
}

impl Generate for SipHash {
    type Event<'a> = TxEvent;

    fn deploy<P>(&mut self, protocol: &mut P)
    where
        P: for<'a> Protocol<Self::Event<'a>, Effect = ()>,
    {
        for (seq, event) in self.channel.iter() {
            let RxEvent::Receive(mut buf) = event.into();
            buf.to_mut()[..4].copy_from_slice(&seq.to_be_bytes());

            let mut signatures = [0; 16];
            let mut index = 0;
            while index < self.replica_count {
                let mut buf = Box::<[_]>::from(buf.clone());
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

impl Protocol<(u32, RxEventOwned)> for P256 {
    type Effect = TxEvent;

    fn update(&mut self, (seq, event): (u32, RxEventOwned)) -> Self::Effect {
        let RxEvent::Receive(mut buf) = event.into();
        buf.to_mut()[..4].copy_from_slice(&seq.to_be_bytes());

        let message = secp256k1::Message::from_slice(&buf[..32]).unwrap();
        let signature = self
            .secp
            .sign_ecdsa(&message, &self.secret_key)
            .serialize_compact();

        buf.to_mut()[4..68].copy_from_slice(&signature);
        TxEvent::Send(self.multicast_addr, buf.into())
    }
}

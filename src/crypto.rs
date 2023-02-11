use std::mem::take;

use bincode::Options;
use secp256k1::{ecdsa, hashes::sha256, All, Message, PublicKey, Secp256k1, SecretKey};
use serde::Serialize;

pub type Signature = ([u8; 32], [u8; 32]);

pub trait CryptoMessage: Serialize {
    fn signature(&mut self) -> Option<&mut Signature>;
}

thread_local! {
    static SECP: Secp256k1<All> = Secp256k1::new();
}

pub fn sign(message: &mut impl CryptoMessage, secret_key: &SecretKey) {
    let digest =
        Message::from_hashed_data::<sha256::Hash>(&bincode::options().serialize(&message).unwrap());
    if let Some(signature) = message.signature() {
        let bytes = SECP
            .with(|secp| secp.sign_ecdsa(&digest, secret_key))
            .serialize_compact();
        signature.0 = bytes[..32].try_into().unwrap();
        signature.1 = bytes[32..].try_into().unwrap();
    }
}

pub fn verify<M>(mut message: M, public_key: &PublicKey) -> Option<M>
where
    M: CryptoMessage,
{
    let signature = if let Some(signature) = message.signature() {
        take(signature)
    } else {
        return Some(message);
    };
    let mut bytes = [0; 64];
    bytes[..32].copy_from_slice(&signature.0);
    bytes[32..].copy_from_slice(&signature.1);
    let digest =
        Message::from_hashed_data::<sha256::Hash>(&bincode::options().serialize(&message).unwrap());
    SECP.with(|secp| {
        secp.verify_ecdsa(
            &digest,
            &ecdsa::Signature::from_compact(&bytes).unwrap(),
            public_key,
        )
    })
    .ok()
    .map(|_| {
        *message.signature().unwrap() = signature;
        message
    })
}

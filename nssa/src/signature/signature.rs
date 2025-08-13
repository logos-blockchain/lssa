use rand::{RngCore, rngs::OsRng};

use crate::{PrivateKey, PublicKey};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub(crate) value: [u8; 64],
}

impl Signature {
    pub fn new(key: &PrivateKey, message: &[u8]) -> Self {
        let mut aux_random = [0u8; 32];
        OsRng.fill_bytes(&mut aux_random);
        Self::new_with_aux_random(key, message, aux_random)
    }

    pub(crate) fn new_with_aux_random(
        key: &PrivateKey,
        message: &[u8],
        aux_random: [u8; 32],
    ) -> Self {
        let value = {
            let secp = secp256k1::Secp256k1::new();
            let secret_key = secp256k1::SecretKey::from_byte_array(key.0).unwrap();
            let keypair = secp256k1::Keypair::from_secret_key(&secp, &secret_key);
            let signature = secp.sign_schnorr_with_aux_rand(message, &keypair, &aux_random);
            signature.to_byte_array()
        };
        Self { value }
    }

    pub fn is_valid_for(&self, bytes: &[u8], public_key: &PublicKey) -> bool {
        let pk = secp256k1::XOnlyPublicKey::from_byte_array(public_key.0).unwrap();
        let secp = secp256k1::Secp256k1::new();
        let sig = secp256k1::schnorr::Signature::from_byte_array(self.value);
        secp.verify_schnorr(&sig, bytes, &pk).is_ok()
    }
}

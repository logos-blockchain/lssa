use std::io::{Cursor, Read};

use serde::{Deserialize, Serialize};

use crate::PrivateKey;

// TODO: Dummy impl. Replace by actual public key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicKey(pub(crate) [u8; 32]);

impl PublicKey {
    pub fn new(key: &PrivateKey) -> Self {
        let value = {
            let secret_key = secp256k1::SecretKey::from_byte_array(key.0).unwrap();
            let public_key =
                secp256k1::PublicKey::from_secret_key(&secp256k1::Secp256k1::new(), &secret_key);
            let (x_only, _) = public_key.x_only_public_key();
            x_only.serialize()
        };
        Self(value)
    }
}

impl PublicKey {
    // TODO: remove unwraps and return Result
    pub(crate) fn from_cursor(cursor: &mut Cursor<&[u8]>) -> Self {
        let mut value = [0u8; 32];
        cursor.read_exact(&mut value).unwrap();
        Self(value)
    }
}

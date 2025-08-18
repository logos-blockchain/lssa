use risc0_zkvm::sha::{Impl, Sha256};
use serde::{Deserialize, Serialize};

use crate::account::Commitment;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NullifierPublicKey(pub(super) [u8; 32]);

impl From<&NullifierSecretKey> for NullifierPublicKey {
    fn from(value: &NullifierSecretKey) -> Self {
        let mut bytes = Vec::new();
        const PREFIX: &[u8; 9] = b"NSSA_keys";
        const SUFFIX_1: &[u8; 1] = &[7];
        const SUFFIX_2: &[u8; 22] = &[0; 22];
        bytes.extend_from_slice(PREFIX);
        bytes.extend_from_slice(value);
        bytes.extend_from_slice(SUFFIX_1);
        bytes.extend_from_slice(SUFFIX_2);
        Self(Impl::hash_bytes(&bytes).as_bytes().try_into().unwrap())
    }
}

pub type NullifierSecretKey = [u8; 32];

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Nullifier([u8; 32]);

impl Nullifier {
    pub fn new(commitment: &Commitment, nsk: &NullifierSecretKey) -> Self {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&commitment.to_byte_array());
        bytes.extend_from_slice(nsk);
        Self(Impl::hash_bytes(&bytes).as_bytes().try_into().unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constructor() {
        let commitment = Commitment((0..32u8).collect::<Vec<_>>().try_into().unwrap());
        let nsk = [0x42; 32];
        let expected_nullifier = Nullifier([
            97, 87, 111, 191, 0, 44, 125, 145, 237, 104, 31, 230, 203, 254, 68, 176, 126, 17, 240,
            205, 249, 143, 11, 43, 15, 198, 189, 219, 191, 49, 36, 61,
        ]);
        let nullifier = Nullifier::new(&commitment, &nsk);
        assert_eq!(nullifier, expected_nullifier);
    }

    #[test]
    fn test_from_secret_key() {
        let nsk = [
            50, 139, 109, 225, 82, 86, 80, 108, 140, 248, 232, 229, 96, 80, 148, 250, 15, 9, 155,
            44, 196, 224, 115, 180, 160, 44, 113, 133, 15, 196, 253, 42,
        ];
        let expected_Npk = NullifierPublicKey([
            38, 90, 215, 216, 195, 66, 157, 77, 161, 59, 121, 18, 118, 37, 57, 199, 189, 251, 95,
            130, 12, 9, 171, 169, 140, 221, 87, 242, 46, 243, 111, 85,
        ]);
        let Npk = NullifierPublicKey::from(&nsk);
        assert_eq!(Npk, expected_Npk);
    }
}

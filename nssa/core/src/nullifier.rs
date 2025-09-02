use risc0_zkvm::sha::{Impl, Sha256};
use serde::{Deserialize, Serialize};

use crate::Commitment;

#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(any(feature = "host", test), derive(Debug, Clone, Hash))]
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

#[derive(Serialize, Deserialize)]
#[cfg_attr(any(feature = "host", test), derive(Debug, Clone, PartialEq, Eq, Hash))]
pub struct Nullifier(pub(super) [u8; 32]);

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
            57, 5, 64, 115, 153, 56, 184, 51, 207, 238, 99, 165, 147, 214, 213, 151, 30, 251, 30,
            196, 134, 22, 224, 211, 237, 120, 136, 225, 188, 220, 249, 28,
        ];
        let expected_npk = NullifierPublicKey([
            202, 120, 42, 189, 194, 218, 78, 244, 31, 6, 108, 169, 29, 61, 22, 221, 69, 138, 197,
            161, 241, 39, 142, 242, 242, 50, 188, 201, 99, 28, 176, 238,
        ]);
        let npk = NullifierPublicKey::from(&nsk);
        assert_eq!(npk, expected_npk);
    }
}

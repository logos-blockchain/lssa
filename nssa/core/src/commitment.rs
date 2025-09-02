use risc0_zkvm::sha::{Impl, Sha256};
use serde::{Deserialize, Serialize};

use crate::{NullifierPublicKey, account::Account};

#[derive(Serialize, Deserialize)]
#[cfg_attr(any(feature = "host", test), derive(Debug, Clone, PartialEq, Eq, Hash))]
pub struct Commitment(pub(super) [u8; 32]);

impl Commitment {
    pub fn new(npk: &NullifierPublicKey, account: &Account) -> Self {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&npk.to_byte_array());
        let account_bytes_with_hashed_data = {
            let mut this = Vec::new();
            for word in &account.program_owner {
                this.extend_from_slice(&word.to_le_bytes());
            }
            this.extend_from_slice(&account.balance.to_le_bytes());
            this.extend_from_slice(&account.nonce.to_le_bytes());
            let hashed_data: [u8; 32] = Impl::hash_bytes(&account.data)
                .as_bytes()
                .try_into()
                .unwrap();
            this.extend_from_slice(&hashed_data);
            this
        };
        bytes.extend_from_slice(&account_bytes_with_hashed_data);
        Self(Impl::hash_bytes(&bytes).as_bytes().try_into().unwrap())
    }
}

pub type CommitmentSetDigest = [u8; 32];

pub type MembershipProof = (usize, Vec<[u8; 32]>);

pub fn compute_digest_for_path(
    commitment: &Commitment,
    proof: &MembershipProof,
) -> CommitmentSetDigest {
    let value_bytes = commitment.to_byte_array();
    let mut result: [u8; 32] = Impl::hash_bytes(&value_bytes)
        .as_bytes()
        .try_into()
        .unwrap();
    let mut level_index = proof.0;
    for node in &proof.1 {
        let is_left_child = level_index & 1 == 0;
        if is_left_child {
            let mut bytes = [0u8; 64];
            bytes[..32].copy_from_slice(&result);
            bytes[32..].copy_from_slice(node);
            result = Impl::hash_bytes(&bytes).as_bytes().try_into().unwrap();
        } else {
            let mut bytes = [0u8; 64];
            bytes[..32].copy_from_slice(node);
            bytes[32..].copy_from_slice(&result);
            result = Impl::hash_bytes(&bytes).as_bytes().try_into().unwrap();
        }
        level_index >>= 1;
    }
    result
}

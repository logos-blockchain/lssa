use risc0_zkvm::{
    serde::to_vec,
    sha::{Impl, Sha256},
};
use serde::{Deserialize, Serialize};

use crate::account::{Account, NullifierPublicKey};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Commitment(pub(super) [u8; 32]);

impl Commitment {
    pub fn new(Npk: &NullifierPublicKey, account: &Account) -> Self {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&Npk.to_byte_array());
        bytes.extend_from_slice(&account.to_bytes());
        Self(Impl::hash_bytes(&bytes).as_bytes().try_into().unwrap())
    }
}

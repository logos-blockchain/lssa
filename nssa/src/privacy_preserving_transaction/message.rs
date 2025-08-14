use nssa_core::account::{Account, Commitment, Nonce, Nullifier};

use crate::Address;

#[derive(Debug, Clone, PartialEq, Eq)]
struct EncryptedAccountData;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub(crate) public_addresses: Vec<Address>,
    pub(crate) nonces: Vec<Nonce>,
    pub(crate) public_post_states: Vec<Account>,
    pub(crate) encrypted_private_post_states: Vec<EncryptedAccountData>,
    pub(crate) new_commitments: Vec<Commitment>,
    pub(crate) new_nullifiers: Vec<Nullifier>,
}

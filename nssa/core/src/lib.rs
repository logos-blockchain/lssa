use risc0_zkvm::serde::to_vec;
use serde::{Deserialize, Serialize};

#[cfg(feature = "host")]
use crate::error::NssaCoreError;

use crate::{
    account::{
        Account, AccountWithMetadata, Commitment, Nonce, Nullifier, NullifierPublicKey,
        NullifierSecretKey,
    },
    program::{ProgramId, ProgramOutput},
};

#[cfg(feature = "host")]
use std::io::Cursor;

pub mod account;
pub mod program;

#[cfg(feature = "host")]
pub mod error;

pub type CommitmentSetDigest = [u32; 8];
pub type MembershipProof = Vec<[u8; 32]>;
pub fn verify_membership_proof(
    commitment: &Commitment,
    proof: &MembershipProof,
    digest: &CommitmentSetDigest,
) -> bool {
    todo!()
}

pub type IncomingViewingPublicKey = [u8; 32];
pub type EphemeralSecretKey = [u8; 32];
pub struct EphemeralPublicKey;

impl From<&EphemeralSecretKey> for EphemeralPublicKey {
    fn from(value: &EphemeralSecretKey) -> Self {
        todo!()
    }
}

pub struct Tag(u8);
impl Tag {
    pub fn new(Npk: &NullifierPublicKey, Ipk: &IncomingViewingPublicKey) -> Self {
        todo!()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct EncryptedAccountData(u8);

impl EncryptedAccountData {
    pub fn new(
        account: &Account,
        esk: &EphemeralSecretKey,
        Npk: &NullifierPublicKey,
        Ivk: &IncomingViewingPublicKey,
    ) -> Self {
        // TODO: implement
        Self(0)
    }

    #[cfg(feature = "host")]
    pub fn from_cursor(cursor: &mut Cursor<&[u8]>) -> Result<Self, NssaCoreError> {
        let dummy_value = EncryptedAccountData(0);
        Ok(dummy_value)
    }
}

impl EncryptedAccountData {
    pub fn to_bytes(&self) -> Vec<u8> {
        // TODO: implement
        vec![0]
    }
}

#[derive(Serialize, Deserialize)]
pub struct PrivacyPreservingCircuitInput {
    pub program_output: ProgramOutput,
    pub visibility_mask: Vec<u8>,
    pub private_account_nonces: Vec<Nonce>,
    pub private_account_keys: Vec<(
        NullifierPublicKey,
        IncomingViewingPublicKey,
        EphemeralSecretKey,
    )>,
    pub private_account_auth: Vec<(NullifierSecretKey, MembershipProof)>,
    pub program_id: ProgramId,
    pub commitment_set_digest: CommitmentSetDigest,
}

#[derive(Serialize, Deserialize)]
pub struct PrivacyPreservingCircuitOutput {
    pub public_pre_states: Vec<AccountWithMetadata>,
    pub public_post_states: Vec<Account>,
    pub encrypted_private_post_states: Vec<EncryptedAccountData>,
    pub new_commitments: Vec<Commitment>,
    pub new_nullifiers: Vec<Nullifier>,
    pub commitment_set_digest: CommitmentSetDigest,
}

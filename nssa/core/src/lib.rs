use risc0_zkvm::{
    serde::to_vec,
    sha::{Impl, Sha256},
};
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

pub type CommitmentSetDigest = [u8; 32];
pub type MembershipProof = (usize, Vec<[u8; 32]>);
pub fn verify_membership_proof(
    commitment: &Commitment,
    proof: &MembershipProof,
    digest: &CommitmentSetDigest,
) -> bool {
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
    &result == digest
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

#[derive(Serialize, Deserialize)]
#[cfg_attr(any(feature = "host", test), derive(Debug, Clone, PartialEq, Eq))]
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
#[cfg_attr(any(feature = "host", test), derive(Debug, PartialEq, Eq))]
pub struct PrivacyPreservingCircuitOutput {
    pub public_pre_states: Vec<AccountWithMetadata>,
    pub public_post_states: Vec<Account>,
    pub encrypted_private_post_states: Vec<EncryptedAccountData>,
    pub new_commitments: Vec<Commitment>,
    pub new_nullifiers: Vec<Nullifier>,
    pub commitment_set_digest: CommitmentSetDigest,
}

#[cfg(feature = "host")]
impl PrivacyPreservingCircuitOutput {
    pub fn to_bytes(&self) -> Vec<u8> {
        bytemuck::cast_slice(&to_vec(&self).unwrap()).to_vec()
    }
}

#[cfg(test)]
mod tests {
    use risc0_zkvm::serde::from_slice;

    use crate::{
        EncryptedAccountData, PrivacyPreservingCircuitOutput,
        account::{Account, AccountWithMetadata, Commitment, Nullifier, NullifierPublicKey},
    };

    #[test]
    fn test_privacy_preserving_circuit_output_to_bytes_is_compatible_with_from_slice() {
        let output = PrivacyPreservingCircuitOutput {
            public_pre_states: vec![
                AccountWithMetadata {
                    account: Account {
                        program_owner: [1, 2, 3, 4, 5, 6, 7, 8],
                        balance: 12345678901234567890,
                        data: b"test data".to_vec(),
                        nonce: 18446744073709551614,
                    },
                    is_authorized: true,
                },
                AccountWithMetadata {
                    account: Account {
                        program_owner: [9, 9, 9, 8, 8, 8, 7, 7],
                        balance: 123123123456456567112,
                        data: b"test data".to_vec(),
                        nonce: 9999999999999999999999,
                    },
                    is_authorized: false,
                },
            ],
            public_post_states: vec![Account {
                program_owner: [1, 2, 3, 4, 5, 6, 7, 8],
                balance: 100,
                data: b"post state data".to_vec(),
                nonce: 18446744073709551615,
            }],
            encrypted_private_post_states: vec![EncryptedAccountData(0)],
            new_commitments: vec![Commitment::new(
                &NullifierPublicKey::from(&[1; 32]),
                &Account::default(),
            )],
            new_nullifiers: vec![Nullifier::new(
                &Commitment::new(&NullifierPublicKey::from(&[2; 32]), &Account::default()),
                &[1; 32],
            )],
            commitment_set_digest: [0xab; 32],
        };
        let bytes = output.to_bytes();
        let output_from_slice: PrivacyPreservingCircuitOutput = from_slice(&bytes).unwrap();
        assert_eq!(output, output_from_slice);
    }
}

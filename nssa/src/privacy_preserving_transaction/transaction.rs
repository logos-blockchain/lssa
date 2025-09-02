use std::collections::{HashMap, HashSet};

use nssa_core::{
    Commitment, CommitmentSetDigest, Nullifier, PrivacyPreservingCircuitOutput,
    account::{Account, AccountWithMetadata},
};

use crate::error::NssaError;
use crate::privacy_preserving_transaction::circuit::Proof;
use crate::privacy_preserving_transaction::message::EncryptedAccountData;
use crate::{Address, V01State};

use super::message::Message;
use super::witness_set::WitnessSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivacyPreservingTransaction {
    message: Message,
    witness_set: WitnessSet,
}

impl PrivacyPreservingTransaction {
    pub fn new(message: Message, witness_set: WitnessSet) -> Self {
        Self {
            message,
            witness_set,
        }
    }

    pub(crate) fn validate_and_produce_public_state_diff(
        &self,
        state: &mut V01State,
    ) -> Result<HashMap<Address, Account>, NssaError> {
        let message = &self.message;
        let witness_set = &self.witness_set;

        // 1. Commitments or nullifiers are non empty
        if message.new_commitments.is_empty() && message.new_nullifiers.is_empty() {
            return Err(NssaError::InvalidInput(
                "Empty commitments and empty nullifiers found in message".into(),
            ));
        }

        // 2. Check there are no duplicate addresses in the public_addresses list.
        if n_unique(&message.public_addresses) != message.public_addresses.len() {
            return Err(NssaError::InvalidInput(
                "Duplicate addresses found in message".into(),
            ));
        }

        // Check there are no duplicate nullifiers in the new_nullifiers list
        if n_unique(&message.new_nullifiers) != message.new_nullifiers.len() {
            return Err(NssaError::InvalidInput(
                "Duplicate nullifiers found in message".into(),
            ));
        }

        // Check there are no duplicate commitments in the new_commitments list
        if n_unique(&message.new_commitments) != message.new_commitments.len() {
            return Err(NssaError::InvalidInput(
                "Duplicate commitments found in message".into(),
            ));
        }

        // 3. Nonce checks and Valid signatures
        // Check exactly one nonce is provided for each signature
        if message.nonces.len() != witness_set.signatures_and_public_keys.len() {
            return Err(NssaError::InvalidInput(
                "Mismatch between number of nonces and signatures/public keys".into(),
            ));
        }

        // Check the signatures are valid
        if !witness_set.signatures_are_valid_for(message) {
            return Err(NssaError::InvalidInput(
                "Invalid signature for given message and public key".into(),
            ));
        }

        let signer_addresses = self.signer_addresses();
        // Check nonces corresponds to the current nonces on the public state.
        for (address, nonce) in signer_addresses.iter().zip(&message.nonces) {
            let current_nonce = state.get_account_by_address(address).nonce;
            if current_nonce != *nonce {
                return Err(NssaError::InvalidInput("Nonce mismatch".into()));
            }
        }

        // Build pre_states for proof verification
        let public_pre_states: Vec<_> = message
            .public_addresses
            .iter()
            .map(|address| AccountWithMetadata {
                account: state.get_account_by_address(address),
                is_authorized: signer_addresses.contains(address),
            })
            .collect();

        // 4. Proof verification
        check_privacy_preserving_circuit_proof_is_valid(
            &witness_set.proof,
            &public_pre_states,
            &message.public_post_states,
            &message.encrypted_private_post_states,
            &message.new_commitments,
            &message.new_nullifiers,
        )?;

        // 5. Commitment freshness
        state.check_commitments_are_new(&message.new_commitments)?;

        // 6. Nullifier uniqueness
        state.check_nullifiers_are_valid(&message.new_nullifiers)?;

        Ok(message
            .public_addresses
            .iter()
            .cloned()
            .zip(message.public_post_states.clone())
            .collect())
    }

    pub fn message(&self) -> &Message {
        &self.message
    }

    pub fn witness_set(&self) -> &WitnessSet {
        &self.witness_set
    }

    pub(crate) fn signer_addresses(&self) -> Vec<Address> {
        self.witness_set
            .signatures_and_public_keys()
            .iter()
            .map(|(_, public_key)| Address::from(public_key))
            .collect()
    }
}

fn check_privacy_preserving_circuit_proof_is_valid(
    proof: &Proof,
    public_pre_states: &[AccountWithMetadata],
    public_post_states: &[Account],
    encrypted_private_post_states: &[EncryptedAccountData],
    new_commitments: &[Commitment],
    new_nullifiers: &[(Nullifier, CommitmentSetDigest)],
) -> Result<(), NssaError> {
    let output = PrivacyPreservingCircuitOutput {
        public_pre_states: public_pre_states.to_vec(),
        public_post_states: public_post_states.to_vec(),
        ciphertexts: encrypted_private_post_states
            .iter()
            .cloned()
            .map(|value| value.ciphertext)
            .collect(),
        new_commitments: new_commitments.to_vec(),
        new_nullifiers: new_nullifiers.to_vec(),
    };
    proof
        .is_valid_for(&output)
        .then_some(())
        .ok_or(NssaError::InvalidPrivacyPreservingProof)
}

use std::hash::Hash;
fn n_unique<T: Eq + Hash>(data: &[T]) -> usize {
    let set: HashSet<&T> = data.iter().collect();
    set.len()
}

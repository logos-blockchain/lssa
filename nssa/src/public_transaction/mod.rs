use std::collections::{HashMap, HashSet};

use nssa_core::{
    account::{Account, AccountWithMetadata},
    program::validate_execution,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, digest::FixedOutput};

use crate::{V01State, address::Address, error::NssaError};

mod message;
mod witness_set;

pub use message::Message;
pub use witness_set::WitnessSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicTransaction {
    message: Message,
    witness_set: WitnessSet,
}

impl PublicTransaction {
    pub fn message(&self) -> &Message {
        &self.message
    }

    pub fn witness_set(&self) -> &WitnessSet {
        &self.witness_set
    }

    pub(crate) fn signer_addresses(&self) -> Vec<Address> {
        self.witness_set
            .signatures_and_public_keys
            .iter()
            .map(|(_, public_key)| Address::from_public_key(public_key))
            .collect()
    }

    pub fn new(message: Message, witness_set: WitnessSet) -> Self {
        Self {
            message,
            witness_set,
        }
    }

    pub fn hash(&self) -> [u8; 32] {
        let bytes = serde_cbor::to_vec(&self).unwrap();
        let mut hasher = sha2::Sha256::new();
        hasher.update(&bytes);
        hasher.finalize_fixed().into()
    }

    pub(crate) fn validate_and_compute_post_states(
        &self,
        state: &V01State,
    ) -> Result<HashMap<Address, Account>, NssaError> {
        let message = self.message();
        let witness_set = self.witness_set();

        // All addresses must be different
        if message.addresses.iter().collect::<HashSet<_>>().len() != message.addresses.len() {
            return Err(NssaError::InvalidInput(
                "Duplicate addresses found in message".into(),
            ));
        }

        if message.nonces.len() != witness_set.signatures_and_public_keys.len() {
            return Err(NssaError::InvalidInput(
                "Mismatch between number of nonces and signatures/public keys".into(),
            ));
        }

        let mut authorized_addresses = Vec::new();
        for ((signature, public_key), nonce) in witness_set.iter_signatures().zip(&message.nonces) {
            // Check the signature is valid
            if !signature.is_valid_for(message, public_key) {
                return Err(NssaError::InvalidInput(
                    "Invalid signature for given message and public key".into(),
                ));
            }

            // Check the nonce corresponds to the current nonce on the public state.
            let address = Address::from_public_key(public_key);
            let current_nonce = state.get_account_by_address(&address).nonce;
            if current_nonce != *nonce {
                return Err(NssaError::InvalidInput("Nonce mismatch".into()));
            }

            authorized_addresses.push(address);
        }

        // Build pre_states for execution
        let pre_states: Vec<_> = message
            .addresses
            .iter()
            .map(|address| AccountWithMetadata {
                account: state.get_account_by_address(address),
                is_authorized: authorized_addresses.contains(address),
            })
            .collect();

        // Check the `program_id` corresponds to a built-in program
        // Only allowed program so far is the authenticated transfer program
        let Some(program) = state.builtin_programs().get(&message.program_id) else {
            return Err(NssaError::InvalidInput("Unknown program".into()));
        };

        // // Execute program
        let post_states = program.execute(&pre_states, &message.instruction_data)?;

        // Verify execution corresponds to a well-behaved program.
        // See the # Programs section for the definition of the `validate_execution` method.
        if !validate_execution(&pre_states, &post_states, message.program_id) {
            return Err(NssaError::InvalidProgramBehavior);
        }

        Ok(message.addresses.iter().cloned().zip(post_states).collect())
    }
}

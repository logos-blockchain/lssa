use std::{
    collections::{HashMap, HashSet},
    io::Cursor,
};

use nssa_core::{
    account::{Account, AccountWithMetadata},
    program::validate_execution,
};
use sha2::{Digest, digest::FixedOutput};

use crate::{V01State, address::Address, error::NssaError};

mod message;
mod witness_set;

pub use message::Message;
pub use witness_set::WitnessSet;

#[derive(Debug, Clone, PartialEq, Eq)]
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

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = self.message.message_to_bytes();
        bytes.extend_from_slice(&self.witness_set.to_bytes());
        bytes
    }

    pub fn from_cursor(cursor: &mut Cursor<&[u8]>) -> Self {
        let message = Message::from_cursor(cursor);
        let witness_set = WitnessSet::from_cursor(cursor);
        Self {
            message,
            witness_set,
        }
    }

    pub fn hash(&self) -> [u8; 32] {
        let bytes = self.to_bytes();
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

        // Check the signatures are valid
        if !witness_set.is_valid_for(message) {
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

        // Build pre_states for execution
        let pre_states: Vec<_> = message
            .addresses
            .iter()
            .map(|address| AccountWithMetadata {
                account: state.get_account_by_address(address),
                is_authorized: signer_addresses.contains(address),
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

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::{
        Address, PrivateKey, PublicTransaction, PublicKey,
        program::Program,
        public_transaction::{Message, WitnessSet},
    };

    #[test]
    fn test() {
        let key1 = PrivateKey::try_new([1; 32]).unwrap();
        let key2 = PrivateKey::try_new([2; 32]).unwrap();
        let addr1 = Address::from_public_key(&PublicKey::new(&key1));
        let addr2 = Address::from_public_key(&PublicKey::new(&key2));
        let nonces = vec![5, 99];
        let instruction = 1337;
        let message = Message::try_new(
            Program::authenticated_transfer_program().id(),
            vec![addr1, addr2],
            nonces,
            instruction,
        )
        .unwrap();

        let witness_set = WitnessSet::for_message(&message, &[&key1, &key2]);
        let tx = PublicTransaction::new(message, witness_set);

        let bytes = tx.to_bytes();
        let mut cursor: Cursor<&[u8]> = Cursor::new(&bytes);
        let recov_tx = PublicTransaction::from_cursor(&mut cursor);
        assert_eq!(tx, recov_tx);
    }
}

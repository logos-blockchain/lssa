use std::collections::{HashMap, HashSet};

use nssa_core::{
    account::{Account, AccountWithMetadata},
    address::Address,
    program::{DEFAULT_PROGRAM_ID, validate_execution},
};
use sha2::{Digest, digest::FixedOutput};

use crate::{
    V02State,
    error::NssaError,
    public_transaction::{Message, WitnessSet},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicTransaction {
    message: Message,
    witness_set: WitnessSet,
}
const MAX_NUMBER_CHAINED_CALLS: usize = 10;

impl PublicTransaction {
    pub fn new(message: Message, witness_set: WitnessSet) -> Self {
        Self {
            message,
            witness_set,
        }
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

    pub fn hash(&self) -> [u8; 32] {
        let bytes = self.to_bytes();
        let mut hasher = sha2::Sha256::new();
        hasher.update(&bytes);
        hasher.finalize_fixed().into()
    }

    pub(crate) fn validate_and_produce_public_state_diff(
        &self,
        state: &V02State,
    ) -> Result<HashMap<Address, Account>, NssaError> {
        let message = self.message();
        let witness_set = self.witness_set();

        // All addresses must be different
        if message.addresses.iter().collect::<HashSet<_>>().len() != message.addresses.len() {
            return Err(NssaError::InvalidInput(
                "Duplicate addresses found in message".into(),
            ));
        }

        // Check exactly one nonce is provided for each signature
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
        let mut input_pre_states: Vec<_> = message
            .addresses
            .iter()
            .map(|address| {
                AccountWithMetadata::new(
                    state.get_account_by_address(address),
                    signer_addresses.contains(address),
                    *address,
                )
            })
            .collect();

        let mut state_diff: HashMap<Address, Account> = HashMap::new();

        let mut program_id = message.program_id;
        let mut instruction_data = message.instruction_data.clone();

        for _i in 0..MAX_NUMBER_CHAINED_CALLS {
            // Check the `program_id` corresponds to a deployed program
            let Some(program) = state.programs().get(&program_id) else {
                return Err(NssaError::InvalidInput("Unknown program".into()));
            };

            let mut program_output = program.execute(&input_pre_states, &instruction_data)?;

            // This check is equivalent to checking that the program output pre_states coinicide
            // with the values in the public state or with any modifications to those values
            // during the chain of calls.
            if input_pre_states != program_output.pre_states {
                return Err(NssaError::InvalidProgramBehavior);
            }

            // Verify execution corresponds to a well-behaved program.
            // See the # Programs section for the definition of the `validate_execution` method.
            if !validate_execution(
                &program_output.pre_states,
                &program_output.post_states,
                program_id,
            ) {
                return Err(NssaError::InvalidProgramBehavior);
            }

            // The invoked program claims the accounts with default program id.
            for post in program_output.post_states.iter_mut() {
                if post.program_owner == DEFAULT_PROGRAM_ID {
                    post.program_owner = program_id;
                }
            }

            // Update the state diff
            for (pre, post) in program_output
                .pre_states
                .iter()
                .zip(program_output.post_states.iter())
            {
                state_diff.insert(pre.account_id, post.clone());
            }

            if let Some(next_chained_call) = program_output.chained_call {
                program_id = next_chained_call.program_id;
                instruction_data = next_chained_call.instruction_data;

                // Build post states with metadata for next call
                let mut post_states_with_metadata = Vec::new();
                for (pre, post) in program_output
                    .pre_states
                    .iter()
                    .zip(program_output.post_states)
                {
                    let mut post_with_metadata = pre.clone();
                    post_with_metadata.account = post.clone();
                    post_states_with_metadata.push(post_with_metadata);
                }

                input_pre_states = next_chained_call
                    .account_indices
                    .iter()
                    .map(|&i| {
                        post_states_with_metadata
                            .get(i)
                            .ok_or_else(|| {
                                NssaError::InvalidInput("Invalid account indices".into())
                            })
                            .cloned()
                    })
                    .collect::<Result<Vec<_>, NssaError>>()?;
            } else {
                break;
            };
        }

        Ok(state_diff)
    }
}

#[cfg(test)]
pub mod tests {
    use sha2::{Digest, digest::FixedOutput};

    use crate::{
        Address, PrivateKey, PublicKey, PublicTransaction, Signature, V02State,
        error::NssaError,
        program::Program,
        public_transaction::{Message, WitnessSet},
    };

    fn keys_for_tests() -> (PrivateKey, PrivateKey, Address, Address) {
        let key1 = PrivateKey::try_new([1; 32]).unwrap();
        let key2 = PrivateKey::try_new([2; 32]).unwrap();
        let addr1 = Address::from(&PublicKey::new_from_private_key(&key1));
        let addr2 = Address::from(&PublicKey::new_from_private_key(&key2));
        (key1, key2, addr1, addr2)
    }

    fn state_for_tests() -> V02State {
        let (_, _, addr1, addr2) = keys_for_tests();
        let initial_data = [(addr1, 10000), (addr2, 20000)];
        V02State::new_with_genesis_accounts(&initial_data, &[])
    }

    fn transaction_for_tests() -> PublicTransaction {
        let (key1, key2, addr1, addr2) = keys_for_tests();
        let nonces = vec![0, 0];
        let instruction = 1337;
        let message = Message::try_new(
            Program::authenticated_transfer_program().id(),
            vec![addr1, addr2],
            nonces,
            instruction,
        )
        .unwrap();

        let witness_set = WitnessSet::for_message(&message, &[&key1, &key2]);
        PublicTransaction::new(message, witness_set)
    }

    #[test]
    fn test_new_constructor() {
        let tx = transaction_for_tests();
        let message = tx.message().clone();
        let witness_set = tx.witness_set().clone();
        let tx_from_constructor = PublicTransaction::new(message.clone(), witness_set.clone());
        assert_eq!(tx_from_constructor.message, message);
        assert_eq!(tx_from_constructor.witness_set, witness_set);
    }

    #[test]
    fn test_message_getter() {
        let tx = transaction_for_tests();
        assert_eq!(&tx.message, tx.message());
    }

    #[test]
    fn test_witness_set_getter() {
        let tx = transaction_for_tests();
        assert_eq!(&tx.witness_set, tx.witness_set());
    }

    #[test]
    fn test_signer_addresses() {
        let tx = transaction_for_tests();
        let expected_signer_addresses = vec![
            Address::new([
                208, 122, 210, 232, 75, 39, 250, 0, 194, 98, 240, 161, 238, 160, 255, 53, 202, 9,
                115, 84, 126, 106, 16, 111, 114, 241, 147, 194, 220, 131, 139, 68,
            ]),
            Address::new([
                231, 174, 119, 197, 239, 26, 5, 153, 147, 68, 175, 73, 159, 199, 138, 23, 5, 57,
                141, 98, 237, 6, 207, 46, 20, 121, 246, 222, 248, 154, 57, 188,
            ]),
        ];
        let signer_addresses = tx.signer_addresses();
        assert_eq!(signer_addresses, expected_signer_addresses);
    }

    #[test]
    fn test_public_transaction_encoding_bytes_roundtrip() {
        let tx = transaction_for_tests();
        let bytes = tx.to_bytes();
        let tx_from_bytes = PublicTransaction::from_bytes(&bytes).unwrap();
        assert_eq!(tx, tx_from_bytes);
    }

    #[test]
    fn test_hash_is_sha256_of_transaction_bytes() {
        let tx = transaction_for_tests();
        let hash = tx.hash();
        let expected_hash: [u8; 32] = {
            let bytes = tx.to_bytes();
            let mut hasher = sha2::Sha256::new();
            hasher.update(&bytes);
            hasher.finalize_fixed().into()
        };
        assert_eq!(hash, expected_hash);
    }

    #[test]
    fn test_address_list_cant_have_duplicates() {
        let (key1, _, addr1, _) = keys_for_tests();
        let state = state_for_tests();
        let nonces = vec![0, 0];
        let instruction = 1337;
        let message = Message::try_new(
            Program::authenticated_transfer_program().id(),
            vec![addr1, addr1],
            nonces,
            instruction,
        )
        .unwrap();

        let witness_set = WitnessSet::for_message(&message, &[&key1, &key1]);
        let tx = PublicTransaction::new(message, witness_set);
        let result = tx.validate_and_produce_public_state_diff(&state);
        assert!(matches!(result, Err(NssaError::InvalidInput(_))))
    }

    #[test]
    fn test_number_of_nonces_must_match_number_of_signatures() {
        let (key1, key2, addr1, addr2) = keys_for_tests();
        let state = state_for_tests();
        let nonces = vec![0];
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
        let result = tx.validate_and_produce_public_state_diff(&state);
        assert!(matches!(result, Err(NssaError::InvalidInput(_))))
    }

    #[test]
    fn test_all_signatures_must_be_valid() {
        let (key1, key2, addr1, addr2) = keys_for_tests();
        let state = state_for_tests();
        let nonces = vec![0, 0];
        let instruction = 1337;
        let message = Message::try_new(
            Program::authenticated_transfer_program().id(),
            vec![addr1, addr2],
            nonces,
            instruction,
        )
        .unwrap();

        let mut witness_set = WitnessSet::for_message(&message, &[&key1, &key2]);
        witness_set.signatures_and_public_keys[0].0 = Signature::new_for_tests([1; 64]);
        let tx = PublicTransaction::new(message, witness_set);
        let result = tx.validate_and_produce_public_state_diff(&state);
        assert!(matches!(result, Err(NssaError::InvalidInput(_))))
    }

    #[test]
    fn test_nonces_must_match_the_state_current_nonces() {
        let (key1, key2, addr1, addr2) = keys_for_tests();
        let state = state_for_tests();
        let nonces = vec![0, 1];
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
        let result = tx.validate_and_produce_public_state_diff(&state);
        assert!(matches!(result, Err(NssaError::InvalidInput(_))))
    }

    #[test]
    fn test_program_id_must_belong_to_bulitin_program_ids() {
        let (key1, key2, addr1, addr2) = keys_for_tests();
        let state = state_for_tests();
        let nonces = vec![0, 0];
        let instruction = 1337;
        let unknown_program_id = [0xdeadbeef; 8];
        let message =
            Message::try_new(unknown_program_id, vec![addr1, addr2], nonces, instruction).unwrap();

        let witness_set = WitnessSet::for_message(&message, &[&key1, &key2]);
        let tx = PublicTransaction::new(message, witness_set);
        let result = tx.validate_and_produce_public_state_diff(&state);
        assert!(matches!(result, Err(NssaError::InvalidInput(_))))
    }
}

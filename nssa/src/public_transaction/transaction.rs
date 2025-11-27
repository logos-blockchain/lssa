use std::collections::{HashMap, HashSet, VecDeque};

use borsh::{BorshDeserialize, BorshSerialize};
use nssa_core::{
    account::{Account, AccountId, AccountWithMetadata},
    program::{ChainedCall, DEFAULT_PROGRAM_ID, validate_execution},
};
use sha2::{Digest, digest::FixedOutput};

use crate::{
    V02State,
    error::NssaError,
    public_transaction::{Message, WitnessSet},
    state::MAX_NUMBER_CHAINED_CALLS,
};

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PublicTransaction {
    message: Message,
    witness_set: WitnessSet,
}

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

    pub(crate) fn signer_account_ids(&self) -> Vec<AccountId> {
        self.witness_set
            .signatures_and_public_keys()
            .iter()
            .map(|(_, public_key)| AccountId::from(public_key))
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
    ) -> Result<HashMap<AccountId, Account>, NssaError> {
        let message = self.message();
        let witness_set = self.witness_set();

        // All account_ids must be different
        if message.account_ids.iter().collect::<HashSet<_>>().len() != message.account_ids.len() {
            return Err(NssaError::InvalidInput(
                "Duplicate account_ids found in message".into(),
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

        let signer_account_ids = self.signer_account_ids();
        // Check nonces corresponds to the current nonces on the public state.
        for (account_id, nonce) in signer_account_ids.iter().zip(&message.nonces) {
            let current_nonce = state.get_account_by_id(account_id).nonce;
            if current_nonce != *nonce {
                return Err(NssaError::InvalidInput("Nonce mismatch".into()));
            }
        }

        // Build pre_states for execution
        let input_pre_states: Vec<_> = message
            .account_ids
            .iter()
            .map(|account_id| {
                AccountWithMetadata::new(
                    state.get_account_by_id(account_id),
                    signer_account_ids.contains(account_id),
                    *account_id,
                )
            })
            .collect();

        let mut state_diff: HashMap<AccountId, Account> = HashMap::new();

        let initial_call = ChainedCall {
            program_id: message.program_id,
            instruction_data: message.instruction_data.clone(),
            pre_states: input_pre_states,
        };

        let mut chained_calls = VecDeque::from_iter([initial_call]);
        let mut chain_calls_counter = 0;

        while let Some(chained_call) = chained_calls.pop_front() {
            if chain_calls_counter > MAX_NUMBER_CHAINED_CALLS {
                return Err(NssaError::MaxChainedCallsDepthExceeded);
            }

            // Check the `program_id` corresponds to a deployed program
            let Some(program) = state.programs().get(&chained_call.program_id) else {
                return Err(NssaError::InvalidInput("Unknown program".into()));
            };

            let mut program_output =
                program.execute(&chained_call.pre_states, &chained_call.instruction_data)?;

            for pre in &program_output.pre_states {
                let account_id = pre.account_id;
                // Check that the program output pre_states coinicide with the values in the public
                // state or with any modifications to those values during the chain of calls.
                let expected_pre = state_diff
                    .get(&account_id)
                    .cloned()
                    .unwrap_or_else(|| state.get_account_by_id(&account_id));
                if pre.account != expected_pre {
                    return Err(NssaError::InvalidProgramBehavior);
                }

                // Check that authorization flags are consistent with the provided ones
                if pre.is_authorized && !signer_account_ids.contains(&account_id) {
                    return Err(NssaError::InvalidProgramBehavior);
                }
            }

            // Verify execution corresponds to a well-behaved program.
            // See the # Programs section for the definition of the `validate_execution` method.
            if !validate_execution(
                &program_output.pre_states,
                &program_output.post_states,
                chained_call.program_id,
            ) {
                return Err(NssaError::InvalidProgramBehavior);
            }

            // The invoked program claims the accounts with default program id.
            for post in program_output.post_states.iter_mut() {
                if post.program_owner == DEFAULT_PROGRAM_ID {
                    post.program_owner = chained_call.program_id;
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

            for new_call in program_output.chained_calls.into_iter().rev() {
                chained_calls.push_front(new_call);
            }

            chain_calls_counter += 1;
        }

        Ok(state_diff)
    }
}

#[cfg(test)]
pub mod tests {
    use sha2::{Digest, digest::FixedOutput};

    use crate::{
        AccountId, PrivateKey, PublicKey, PublicTransaction, Signature, V02State,
        error::NssaError,
        program::Program,
        public_transaction::{Message, WitnessSet},
    };

    fn keys_for_tests() -> (PrivateKey, PrivateKey, AccountId, AccountId) {
        let key1 = PrivateKey::try_new([1; 32]).unwrap();
        let key2 = PrivateKey::try_new([2; 32]).unwrap();
        let addr1 = AccountId::from(&PublicKey::new_from_private_key(&key1));
        let addr2 = AccountId::from(&PublicKey::new_from_private_key(&key2));
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
    fn test_signer_account_ids() {
        let tx = transaction_for_tests();
        let expected_signer_account_ids = vec![
            AccountId::new([
                208, 122, 210, 232, 75, 39, 250, 0, 194, 98, 240, 161, 238, 160, 255, 53, 202, 9,
                115, 84, 126, 106, 16, 111, 114, 241, 147, 194, 220, 131, 139, 68,
            ]),
            AccountId::new([
                231, 174, 119, 197, 239, 26, 5, 153, 147, 68, 175, 73, 159, 199, 138, 23, 5, 57,
                141, 98, 237, 6, 207, 46, 20, 121, 246, 222, 248, 154, 57, 188,
            ]),
        ];
        let signer_account_ids = tx.signer_account_ids();
        assert_eq!(signer_account_ids, expected_signer_account_ids);
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
    fn test_account_id_list_cant_have_duplicates() {
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

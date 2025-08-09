use crate::{
    AUTHENTICATED_TRANSFER_PROGRAM, address::Address, error::NssaError, execute_public,
    public_transaction::PublicTransaction,
};
use nssa_core::{
    account::{Account, AccountWithMetadata},
    program::{Program, ProgramId, validate_constraints},
};
use std::collections::{HashMap, HashSet};

pub struct V01State {
    public_state: HashMap<Address, Account>,
    builtin_programs: HashMap<ProgramId, Program>,
}

impl V01State {
    pub fn new_with_genesis_accounts(initial_data: &[([u8; 32], u128)]) -> Self {
        let public_state = initial_data
            .iter()
            .copied()
            .map(|(address_value, balance)| {
                let account = Account {
                    balance,
                    program_owner: AUTHENTICATED_TRANSFER_PROGRAM.id,
                    ..Account::default()
                };
                let address = Address::new(address_value);
                (address, account)
            })
            .collect();

        let builtin_programs = HashMap::from([(
            AUTHENTICATED_TRANSFER_PROGRAM.id,
            AUTHENTICATED_TRANSFER_PROGRAM,
        )]);

        Self {
            public_state,
            builtin_programs,
        }
    }

    pub fn transition_from_public_transaction(
        &mut self,
        tx: &PublicTransaction,
    ) -> Result<(), NssaError> {
        let state_diff = self.execute_and_verify_public_transaction(tx)?;

        for (address, post) in state_diff.into_iter() {
            let current_account = self.get_account_by_address_mut(address);
            *current_account = post;
        }

        for address in tx.signer_addresses() {
            let current_account = self.get_account_by_address_mut(address);
            current_account.nonce += 1;
        }
        Ok(())
    }

    fn get_account_by_address_mut(&mut self, address: Address) -> &mut Account {
        self.public_state.entry(address).or_default()
    }

    pub fn get_account_by_address(&self, address: &Address) -> Account {
        self.public_state
            .get(address)
            .cloned()
            .unwrap_or(Account::default())
    }

    fn execute_and_verify_public_transaction(
        &mut self,
        tx: &PublicTransaction,
    ) -> Result<HashMap<Address, Account>, NssaError> {
        let message = tx.message();
        let witness_set = tx.witness_set();

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
        for ((signature, public_key), nonce) in witness_set
            .signatures_and_public_keys
            .iter()
            .zip(message.nonces.iter())
        {
            // Check the signature is valid
            if !signature.is_valid_for(message, public_key) {
                return Err(NssaError::InvalidInput(
                    "Invalid signature for given message and public key".into(),
                ));
            }

            // Check the nonce corresponds to the current nonce on the public state.
            let address = Address::from_public_key(public_key);
            let current_nonce = self.get_account_by_address(&address).nonce;
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
                account: self.get_account_by_address(address),
                is_authorized: authorized_addresses.contains(address),
            })
            .collect();

        // Check the `program_id` corresponds to a built-in program
        // Only allowed program so far is the authenticated transfer program
        let Some(program) = self.builtin_programs.get(&message.program_id) else {
            return Err(NssaError::InvalidInput("Unknown program".into()));
        };

        // // Execute program
        let post_states = execute_public(&pre_states, message.instruction_data, program)?;

        // Verify execution corresponds to a well-behaved program.
        // See the # Programs section for the definition of the `validate_constraints` method.
        if !validate_constraints(&pre_states, &post_states, message.program_id) {
            return Err(NssaError::InvalidProgramBehavior);
        }

        Ok(message.addresses.iter().cloned().zip(post_states).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{public_transaction, signature::PrivateKey};

    fn transfer_transaction_for_tests(
        from: Address,
        from_key: PrivateKey,
        nonce: u128,
        to: Address,
        balance: u128,
    ) -> PublicTransaction {
        let addresses = vec![from, to];
        let nonces = vec![nonce];
        let program_id = AUTHENTICATED_TRANSFER_PROGRAM.id;
        let message = public_transaction::Message::new(program_id, addresses, nonces, balance);
        let witness_set = public_transaction::WitnessSet::for_message(&message, &[&from_key]);
        PublicTransaction::new(message, witness_set)
    }

    #[test]
    fn test_1() {
        let initial_data = [([1; 32], 100)];
        let mut state = V01State::new_with_genesis_accounts(&initial_data);
        let from = Address::new(initial_data[0].0);
        let from_key = PrivateKey(1);
        let to = Address::new([2; 32]);
        let balance_to_move = 5;
        let tx =
            transfer_transaction_for_tests(from.clone(), from_key, 0, to.clone(), balance_to_move);
        state.transition_from_public_transaction(&tx).unwrap();

        assert_eq!(state.get_account_by_address(&to).balance, 5);
        assert_eq!(state.get_account_by_address(&from).balance, 95);
        assert_eq!(state.get_account_by_address(&from).nonce, 1);
        assert_eq!(state.get_account_by_address(&to).nonce, 0);
    }

    #[test]
    fn test_2() {
        let initial_data = [([1; 32], 100), ([99; 32], 200)];
        let mut state = V01State::new_with_genesis_accounts(&initial_data);
        let from = Address::new(initial_data[1].0);
        let from_key = PrivateKey(99);
        let to = Address::new(initial_data[0].0);
        let balance_to_move = 8;
        let tx =
            transfer_transaction_for_tests(from.clone(), from_key, 0, to.clone(), balance_to_move);
        state.transition_from_public_transaction(&tx).unwrap();

        assert_eq!(state.get_account_by_address(&to).balance, 108);
        assert_eq!(state.get_account_by_address(&from).balance, 192);
        assert_eq!(state.get_account_by_address(&from).nonce, 1);
        assert_eq!(state.get_account_by_address(&to).nonce, 0);
    }

    #[test]
    fn test_3() {
        let initial_data = [([1; 32], 100)];
        let mut state = V01State::new_with_genesis_accounts(&initial_data);
        let address_1 = Address::new(initial_data[0].0);
        let key_1 = PrivateKey(1);
        let address_2 = Address::new([2; 32]);

        let key_2 = PrivateKey(2);
        let address_3 = Address::new([3; 32]);

        let balance_to_move = 5;
        let tx = transfer_transaction_for_tests(
            address_1.clone(),
            key_1,
            0,
            address_2.clone(),
            balance_to_move,
        );
        state.transition_from_public_transaction(&tx).unwrap();

        let balance_to_move = 3;
        let tx = transfer_transaction_for_tests(
            address_2.clone(),
            key_2,
            0,
            address_3.clone(),
            balance_to_move,
        );
        state.transition_from_public_transaction(&tx).unwrap();

        assert_eq!(state.get_account_by_address(&address_1).balance, 95);
        assert_eq!(state.get_account_by_address(&address_2).balance, 2);
        assert_eq!(state.get_account_by_address(&address_3).balance, 3);

        assert_eq!(state.get_account_by_address(&address_1).nonce, 1);
        assert_eq!(state.get_account_by_address(&address_2).nonce, 1);
        assert_eq!(state.get_account_by_address(&address_3).nonce, 0);
    }
}

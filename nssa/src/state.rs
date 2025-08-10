use crate::{
    address::Address, error::NssaError, program::Program, public_transaction::PublicTransaction,
};
use nssa_core::{
    account::{Account, AccountWithMetadata},
    program::{ProgramId, validate_execution},
};
use std::collections::{HashMap, HashSet};

pub struct V01State {
    public_state: HashMap<Address, Account>,
    builtin_programs: HashMap<ProgramId, Program>,
}

impl V01State {
    pub fn new_with_genesis_accounts(initial_data: &[([u8; 32], u128)]) -> Self {
        let authenticated_transfer_program = Program::authenticated_transfer_program();
        let public_state = initial_data
            .iter()
            .copied()
            .map(|(address_value, balance)| {
                let account = Account {
                    balance,
                    program_owner: authenticated_transfer_program.id(),
                    ..Account::default()
                };
                let address = Address::new(address_value);
                (address, account)
            })
            .collect();

        let mut this = Self {
            public_state,
            builtin_programs: HashMap::new(),
        };

        this.insert_program(Program::authenticated_transfer_program());

        this
    }

    fn insert_program(&mut self, program: Program) {
        self.builtin_programs.insert(program.id(), program);
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
        for ((signature, public_key), nonce) in witness_set.iter_signatures().zip(&message.nonces) {
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
        let post_states = program.execute(&pre_states, message.instruction_data)?;

        // Verify execution corresponds to a well-behaved program.
        // See the # Programs section for the definition of the `validate_execution` method.
        if !validate_execution(&pre_states, &post_states, message.program_id) {
            return Err(NssaError::InvalidProgramBehavior);
        }

        Ok(message.addresses.iter().cloned().zip(post_states).collect())
    }
}

// Test utils
#[cfg(test)]
impl V01State {
    /// Include test programs in the builtin programs map
    pub fn with_test_programs(mut self) -> Self {
        self.insert_program(Program::nonce_changer_program());
        self.insert_program(Program::extra_output_program());
        self.insert_program(Program::missing_output_program());
        self.insert_program(Program::program_owner_changer());
        self
    }

    pub fn with_non_default_accounts_but_default_program_owners(mut self) -> Self {
        let account_with_default_values_except_balance = Account {
            balance: 100,
            ..Account::default()
        };
        let account_with_default_values_except_nonce = Account {
            nonce: 37,
            ..Account::default()
        };
        let account_with_default_values_except_data = Account {
            data: vec![0xca, 0xfe],
            ..Account::default()
        };
        self.public_state.insert(
            Address::new([255; 32]),
            account_with_default_values_except_balance,
        );
        self.public_state.insert(
            Address::new([254; 32]),
            account_with_default_values_except_nonce,
        );
        self.public_state.insert(
            Address::new([253; 32]),
            account_with_default_values_except_data,
        );
        self
    }
}

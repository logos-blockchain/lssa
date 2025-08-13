use crate::{
    address::Address, error::NssaError, program::Program, public_transaction::PublicTransaction,
};
use nssa_core::{account::Account, program::ProgramId};
use std::collections::HashMap;

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

    pub(crate) fn insert_program(&mut self, program: Program) {
        self.builtin_programs.insert(program.id(), program);
    }

    pub fn transition_from_public_transaction(
        &mut self,
        tx: &PublicTransaction,
    ) -> Result<(), NssaError> {
        let state_diff = tx.validate_and_compute_post_states(self)?;

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

    pub(crate) fn builtin_programs(&self) -> &HashMap<ProgramId, Program> {
        &self.builtin_programs
    }

    #[cfg(test)]
    pub fn force_insert_account(&mut self, address: Address, account: Account) {
        self.public_state.insert(address, account);
    }
}

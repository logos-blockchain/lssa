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
    pub fn new_with_genesis_accounts(initial_data: &[(Address, u128)]) -> Self {
        let authenticated_transfer_program = Program::authenticated_transfer_program();
        let public_state = initial_data
            .iter()
            .copied()
            .map(|(address, balance)| {
                let account = Account {
                    balance,
                    program_owner: authenticated_transfer_program.id(),
                    ..Account::default()
                };
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
}

#[cfg(test)]
mod tests {

    use std::collections::HashMap;

    use crate::{
        Address, PublicKey, PublicTransaction, V01State, error::NssaError, program::Program,
        public_transaction, signature::PrivateKey,
    };
    use nssa_core::account::Account;

    fn transfer_transaction(
        from: Address,
        from_key: PrivateKey,
        nonce: u128,
        to: Address,
        balance: u128,
    ) -> PublicTransaction {
        let addresses = vec![from, to];
        let nonces = vec![nonce];
        let program_id = Program::authenticated_transfer_program().id();
        let message =
            public_transaction::Message::try_new(program_id, addresses, nonces, balance).unwrap();
        let witness_set = public_transaction::WitnessSet::for_message(&message, &[&from_key]);
        PublicTransaction::new(message, witness_set)
    }

    #[test]
    fn test_new_with_genesis() {
        let key1 = PrivateKey::try_new([1; 32]).unwrap();
        let key2 = PrivateKey::try_new([2; 32]).unwrap();
        let addr1 = Address::from(&PublicKey::new_from_private_key(&key1));
        let addr2 = Address::from(&PublicKey::new_from_private_key(&key2));
        let initial_data = [(addr1, 100u128), (addr2, 151u128)];
        let program = Program::authenticated_transfer_program();
        let expected_public_state = {
            let mut this = HashMap::new();
            this.insert(
                addr1,
                Account {
                    balance: 100,
                    program_owner: program.id(),
                    ..Account::default()
                },
            );
            this.insert(
                addr2,
                Account {
                    balance: 151,
                    program_owner: program.id(),
                    ..Account::default()
                },
            );
            this
        };
        let expected_builtin_programs = {
            let mut this = HashMap::new();
            this.insert(program.id(), program);
            this
        };

        let state = V01State::new_with_genesis_accounts(&initial_data);

        assert_eq!(state.public_state, expected_public_state);
        assert_eq!(state.builtin_programs, expected_builtin_programs);
    }

    #[test]
    fn test_insert_program() {
        let mut state = V01State::new_with_genesis_accounts(&[]);
        let program_to_insert = Program::simple_balance_transfer();
        let program_id = program_to_insert.id();
        assert!(!state.builtin_programs.contains_key(&program_id));

        state.insert_program(program_to_insert);

        assert!(state.builtin_programs.contains_key(&program_id));
    }

    #[test]
    fn test_get_account_by_address_non_default_account() {
        let key = PrivateKey::try_new([1; 32]).unwrap();
        let addr = Address::from(&PublicKey::new_from_private_key(&key));
        let initial_data = [(addr, 100u128)];
        let state = V01State::new_with_genesis_accounts(&initial_data);
        let expected_account = state.public_state.get(&addr).unwrap();

        let account = state.get_account_by_address(&addr);

        assert_eq!(&account, expected_account);
    }

    #[test]
    fn test_get_account_by_address_default_account() {
        let addr2 = Address::new([0; 32]);
        let state = V01State::new_with_genesis_accounts(&[]);
        let expected_account = Account::default();

        let account = state.get_account_by_address(&addr2);

        assert_eq!(account, expected_account);
    }

    #[test]
    fn test_builtin_programs_getter() {
        let state = V01State::new_with_genesis_accounts(&[]);

        let builtin_programs = state.builtin_programs();

        assert_eq!(builtin_programs, &state.builtin_programs);
    }

    #[test]
    fn transition_from_authenticated_transfer_program_invocation_default_account_destination() {
        let key = PrivateKey::try_new([1; 32]).unwrap();
        let address = Address::from(&PublicKey::new_from_private_key(&key));
        let initial_data = [(address, 100)];
        let mut state = V01State::new_with_genesis_accounts(&initial_data);
        let from = address;
        let to = Address::new([2; 32]);
        assert_eq!(state.get_account_by_address(&to), Account::default());
        let balance_to_move = 5;

        let tx = transfer_transaction(from.clone(), key, 0, to.clone(), balance_to_move);
        state.transition_from_public_transaction(&tx).unwrap();

        assert_eq!(state.get_account_by_address(&from).balance, 95);
        assert_eq!(state.get_account_by_address(&to).balance, 5);
        assert_eq!(state.get_account_by_address(&from).nonce, 1);
        assert_eq!(state.get_account_by_address(&to).nonce, 0);
    }

    #[test]
    fn transition_from_authenticated_transfer_program_invocation_insuficient_balance() {
        let key = PrivateKey::try_new([1; 32]).unwrap();
        let address = Address::from(&PublicKey::new_from_private_key(&key));
        let initial_data = [(address, 100)];
        let mut state = V01State::new_with_genesis_accounts(&initial_data);
        let from = address;
        let from_key = key;
        let to = Address::new([2; 32]);
        let balance_to_move = 101;
        assert!(state.get_account_by_address(&from).balance < balance_to_move);

        let tx = transfer_transaction(from.clone(), from_key, 0, to.clone(), balance_to_move);
        let result = state.transition_from_public_transaction(&tx);

        assert!(matches!(result, Err(NssaError::ProgramExecutionFailed(_))));
        assert_eq!(state.get_account_by_address(&from).balance, 100);
        assert_eq!(state.get_account_by_address(&to).balance, 0);
        assert_eq!(state.get_account_by_address(&from).nonce, 0);
        assert_eq!(state.get_account_by_address(&to).nonce, 0);
    }

    #[test]
    fn transition_from_authenticated_transfer_program_invocation_non_default_account_destination() {
        let key1 = PrivateKey::try_new([1; 32]).unwrap();
        let key2 = PrivateKey::try_new([2; 32]).unwrap();
        let address1 = Address::from(&PublicKey::new_from_private_key(&key1));
        let address2 = Address::from(&PublicKey::new_from_private_key(&key2));
        let initial_data = [(address1, 100), (address2, 200)];
        let mut state = V01State::new_with_genesis_accounts(&initial_data);
        let from = address2;
        let from_key = key2;
        let to = address1;
        assert_ne!(state.get_account_by_address(&to), Account::default());
        let balance_to_move = 8;

        let tx = transfer_transaction(from.clone(), from_key, 0, to.clone(), balance_to_move);
        state.transition_from_public_transaction(&tx).unwrap();

        assert_eq!(state.get_account_by_address(&from).balance, 192);
        assert_eq!(state.get_account_by_address(&to).balance, 108);
        assert_eq!(state.get_account_by_address(&from).nonce, 1);
        assert_eq!(state.get_account_by_address(&to).nonce, 0);
    }

    #[test]
    fn transition_from_chained_authenticated_transfer_program_invocations() {
        let key1 = PrivateKey::try_new([8; 32]).unwrap();
        let address1 = Address::from(&PublicKey::new_from_private_key(&key1));
        let key2 = PrivateKey::try_new([2; 32]).unwrap();
        let address2 = Address::from(&PublicKey::new_from_private_key(&key2));
        let initial_data = [(address1, 100)];
        let mut state = V01State::new_with_genesis_accounts(&initial_data);
        let address3 = Address::new([3; 32]);
        let balance_to_move = 5;

        let tx = transfer_transaction(address1.clone(), key1, 0, address2.clone(), balance_to_move);
        state.transition_from_public_transaction(&tx).unwrap();
        let balance_to_move = 3;
        let tx = transfer_transaction(address2.clone(), key2, 0, address3.clone(), balance_to_move);
        state.transition_from_public_transaction(&tx).unwrap();

        assert_eq!(state.get_account_by_address(&address1).balance, 95);
        assert_eq!(state.get_account_by_address(&address2).balance, 2);
        assert_eq!(state.get_account_by_address(&address3).balance, 3);
        assert_eq!(state.get_account_by_address(&address1).nonce, 1);
        assert_eq!(state.get_account_by_address(&address2).nonce, 1);
        assert_eq!(state.get_account_by_address(&address3).nonce, 0);
    }

    impl V01State {
        pub fn force_insert_account(&mut self, address: Address, account: Account) {
            self.public_state.insert(address, account);
        }

        /// Include test programs in the builtin programs map
        pub fn with_test_programs(mut self) -> Self {
            self.insert_program(Program::nonce_changer_program());
            self.insert_program(Program::extra_output_program());
            self.insert_program(Program::missing_output_program());
            self.insert_program(Program::program_owner_changer());
            self.insert_program(Program::simple_balance_transfer());
            self.insert_program(Program::data_changer());
            self.insert_program(Program::minter());
            self.insert_program(Program::burner());
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
            self.force_insert_account(
                Address::new([255; 32]),
                account_with_default_values_except_balance,
            );
            self.force_insert_account(
                Address::new([254; 32]),
                account_with_default_values_except_nonce,
            );
            self.force_insert_account(
                Address::new([253; 32]),
                account_with_default_values_except_data,
            );
            self
        }

        pub fn with_account_owned_by_burner_program(mut self) -> Self {
            let account = Account {
                program_owner: Program::burner().id(),
                balance: 100,
                ..Default::default()
            };
            self.force_insert_account(Address::new([252; 32]), account);
            self
        }
    }

    #[test]
    fn test_program_should_fail_if_modifies_nonces() {
        let initial_data = [(Address::new([1; 32]), 100)];
        let mut state = V01State::new_with_genesis_accounts(&initial_data).with_test_programs();
        let addresses = vec![Address::new([1; 32])];
        let program_id = Program::nonce_changer_program().id();
        let message =
            public_transaction::Message::try_new(program_id, addresses, vec![], ()).unwrap();
        let witness_set = public_transaction::WitnessSet::for_message(&message, &[]);
        let tx = PublicTransaction::new(message, witness_set);

        let result = state.transition_from_public_transaction(&tx);

        assert!(matches!(result, Err(NssaError::InvalidProgramBehavior)));
    }

    #[test]
    fn test_program_should_fail_if_output_accounts_exceed_inputs() {
        let initial_data = [(Address::new([1; 32]), 100)];
        let mut state = V01State::new_with_genesis_accounts(&initial_data).with_test_programs();
        let addresses = vec![Address::new([1; 32])];
        let program_id = Program::extra_output_program().id();
        let message =
            public_transaction::Message::try_new(program_id, addresses, vec![], ()).unwrap();
        let witness_set = public_transaction::WitnessSet::for_message(&message, &[]);
        let tx = PublicTransaction::new(message, witness_set);

        let result = state.transition_from_public_transaction(&tx);

        assert!(matches!(result, Err(NssaError::InvalidProgramBehavior)));
    }

    #[test]
    fn test_program_should_fail_with_missing_output_accounts() {
        let initial_data = [(Address::new([1; 32]), 100)];
        let mut state = V01State::new_with_genesis_accounts(&initial_data).with_test_programs();
        let addresses = vec![Address::new([1; 32]), Address::new([2; 32])];
        let program_id = Program::missing_output_program().id();
        let message =
            public_transaction::Message::try_new(program_id, addresses, vec![], ()).unwrap();
        let witness_set = public_transaction::WitnessSet::for_message(&message, &[]);
        let tx = PublicTransaction::new(message, witness_set);

        let result = state.transition_from_public_transaction(&tx);

        assert!(matches!(result, Err(NssaError::InvalidProgramBehavior)));
    }

    #[test]
    fn test_program_should_fail_if_modifies_program_owner_with_only_non_default_program_owner() {
        let initial_data = [(Address::new([1; 32]), 0)];
        let mut state = V01State::new_with_genesis_accounts(&initial_data).with_test_programs();
        let address = Address::new([1; 32]);
        let account = state.get_account_by_address(&address);
        // Assert the target account only differs from the default account in the program owner field
        assert_ne!(account.program_owner, Account::default().program_owner);
        assert_eq!(account.balance, Account::default().balance);
        assert_eq!(account.nonce, Account::default().nonce);
        assert_eq!(account.data, Account::default().data);
        let program_id = Program::program_owner_changer().id();
        let message =
            public_transaction::Message::try_new(program_id, vec![address], vec![], ()).unwrap();
        let witness_set = public_transaction::WitnessSet::for_message(&message, &[]);
        let tx = PublicTransaction::new(message, witness_set);

        let result = state.transition_from_public_transaction(&tx);

        assert!(matches!(result, Err(NssaError::InvalidProgramBehavior)));
    }

    #[test]
    fn test_program_should_fail_if_modifies_program_owner_with_only_non_default_balance() {
        let initial_data = [];
        let mut state = V01State::new_with_genesis_accounts(&initial_data)
            .with_test_programs()
            .with_non_default_accounts_but_default_program_owners();
        let address = Address::new([255; 32]);
        let account = state.get_account_by_address(&address);
        // Assert the target account only differs from the default account in balance field
        assert_eq!(account.program_owner, Account::default().program_owner);
        assert_ne!(account.balance, Account::default().balance);
        assert_eq!(account.nonce, Account::default().nonce);
        assert_eq!(account.data, Account::default().data);
        let program_id = Program::program_owner_changer().id();
        let message =
            public_transaction::Message::try_new(program_id, vec![address], vec![], ()).unwrap();
        let witness_set = public_transaction::WitnessSet::for_message(&message, &[]);
        let tx = PublicTransaction::new(message, witness_set);

        let result = state.transition_from_public_transaction(&tx);

        assert!(matches!(result, Err(NssaError::InvalidProgramBehavior)));
    }

    #[test]
    fn test_program_should_fail_if_modifies_program_owner_with_only_non_default_nonce() {
        let initial_data = [];
        let mut state = V01State::new_with_genesis_accounts(&initial_data)
            .with_test_programs()
            .with_non_default_accounts_but_default_program_owners();
        let address = Address::new([254; 32]);
        let account = state.get_account_by_address(&address);
        // Assert the target account only differs from the default account in nonce field
        assert_eq!(account.program_owner, Account::default().program_owner);
        assert_eq!(account.balance, Account::default().balance);
        assert_ne!(account.nonce, Account::default().nonce);
        assert_eq!(account.data, Account::default().data);
        let program_id = Program::program_owner_changer().id();
        let message =
            public_transaction::Message::try_new(program_id, vec![address], vec![], ()).unwrap();
        let witness_set = public_transaction::WitnessSet::for_message(&message, &[]);
        let tx = PublicTransaction::new(message, witness_set);

        let result = state.transition_from_public_transaction(&tx);

        assert!(matches!(result, Err(NssaError::InvalidProgramBehavior)));
    }

    #[test]
    fn test_program_should_fail_if_modifies_program_owner_with_only_non_default_data() {
        let initial_data = [];
        let mut state = V01State::new_with_genesis_accounts(&initial_data)
            .with_test_programs()
            .with_non_default_accounts_but_default_program_owners();
        let address = Address::new([253; 32]);
        let account = state.get_account_by_address(&address);
        // Assert the target account only differs from the default account in data field
        assert_eq!(account.program_owner, Account::default().program_owner);
        assert_eq!(account.balance, Account::default().balance);
        assert_eq!(account.nonce, Account::default().nonce);
        assert_ne!(account.data, Account::default().data);
        let program_id = Program::program_owner_changer().id();
        let message =
            public_transaction::Message::try_new(program_id, vec![address], vec![], ()).unwrap();
        let witness_set = public_transaction::WitnessSet::for_message(&message, &[]);
        let tx = PublicTransaction::new(message, witness_set);

        let result = state.transition_from_public_transaction(&tx);

        assert!(matches!(result, Err(NssaError::InvalidProgramBehavior)));
    }

    #[test]
    fn test_program_should_fail_if_transfers_balance_from_non_owned_account() {
        let initial_data = [(Address::new([1; 32]), 100)];
        let mut state = V01State::new_with_genesis_accounts(&initial_data).with_test_programs();
        let sender_address = Address::new([1; 32]);
        let receiver_address = Address::new([2; 32]);
        let balance_to_move: u128 = 1;
        let program_id = Program::simple_balance_transfer().id();
        assert_ne!(
            state.get_account_by_address(&sender_address).program_owner,
            program_id
        );
        let message = public_transaction::Message::try_new(
            program_id,
            vec![sender_address, receiver_address],
            vec![],
            balance_to_move,
        )
        .unwrap();
        let witness_set = public_transaction::WitnessSet::for_message(&message, &[]);
        let tx = PublicTransaction::new(message, witness_set);

        let result = state.transition_from_public_transaction(&tx);

        assert!(matches!(result, Err(NssaError::InvalidProgramBehavior)));
    }

    #[test]
    fn test_program_should_fail_if_modifies_data_of_non_owned_account() {
        let initial_data = [];
        let mut state = V01State::new_with_genesis_accounts(&initial_data).with_test_programs();
        let address = Address::new([1; 32]);
        let program_id = Program::data_changer().id();

        // Consider the extreme case where the target account is the default account
        assert_eq!(state.get_account_by_address(&address), Account::default());
        assert_ne!(
            state.get_account_by_address(&address).program_owner,
            program_id
        );
        let message =
            public_transaction::Message::try_new(program_id, vec![address], vec![], ()).unwrap();
        let witness_set = public_transaction::WitnessSet::for_message(&message, &[]);
        let tx = PublicTransaction::new(message, witness_set);

        let result = state.transition_from_public_transaction(&tx);

        assert!(matches!(result, Err(NssaError::InvalidProgramBehavior)));
    }

    #[test]
    fn test_program_should_fail_if_does_not_preserve_total_balance_by_minting() {
        let initial_data = [];
        let mut state = V01State::new_with_genesis_accounts(&initial_data).with_test_programs();
        let address = Address::new([1; 32]);
        let program_id = Program::minter().id();

        let message =
            public_transaction::Message::try_new(program_id, vec![address], vec![], ()).unwrap();
        let witness_set = public_transaction::WitnessSet::for_message(&message, &[]);
        let tx = PublicTransaction::new(message, witness_set);

        let result = state.transition_from_public_transaction(&tx);

        assert!(matches!(result, Err(NssaError::InvalidProgramBehavior)));
    }

    #[test]
    fn test_program_should_fail_if_does_not_preserve_total_balance_by_burning() {
        let initial_data = [];
        let mut state = V01State::new_with_genesis_accounts(&initial_data)
            .with_test_programs()
            .with_account_owned_by_burner_program();
        let program_id = Program::burner().id();
        let address = Address::new([252; 32]);
        assert_eq!(
            state.get_account_by_address(&address).program_owner,
            program_id
        );
        let balance_to_burn: u128 = 1;
        assert!(state.get_account_by_address(&address).balance > balance_to_burn);

        let message = public_transaction::Message::try_new(
            program_id,
            vec![address],
            vec![],
            balance_to_burn,
        )
        .unwrap();
        let witness_set = public_transaction::WitnessSet::for_message(&message, &[]);
        let tx = PublicTransaction::new(message, witness_set);
        let result = state.transition_from_public_transaction(&tx);

        assert!(matches!(result, Err(NssaError::InvalidProgramBehavior)));
    }
}

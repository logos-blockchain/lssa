use crate::{
    error::NssaError, merkle_tree::MerkleTree,
    privacy_preserving_transaction::PrivacyPreservingTransaction, program::Program,
    program_deployment_transaction::ProgramDeploymentTransaction,
    public_transaction::PublicTransaction,
};
use nssa_core::{
    Commitment, CommitmentSetDigest, DUMMY_COMMITMENT, MembershipProof, Nullifier,
    account::Account,
    address::Address,
    program::{DEFAULT_PROGRAM_ID, ProgramId},
};
use std::collections::{HashMap, HashSet};

pub(crate) struct CommitmentSet {
    merkle_tree: MerkleTree,
    commitments: HashMap<Commitment, usize>,
    root_history: HashSet<CommitmentSetDigest>,
}

impl CommitmentSet {
    pub(crate) fn digest(&self) -> CommitmentSetDigest {
        self.merkle_tree.root()
    }

    /// Queries the `CommitmentSet` for a membership proof of commitment
    pub fn get_proof_for(&self, commitment: &Commitment) -> Option<MembershipProof> {
        let index = *self.commitments.get(commitment)?;

        self.merkle_tree
            .get_authentication_path_for(index)
            .map(|path| (index, path))
    }

    /// Inserts a list of commitments to the `CommitmentSet`.
    pub(crate) fn extend(&mut self, commitments: &[Commitment]) {
        for commitment in commitments.iter().cloned() {
            let index = self.merkle_tree.insert(commitment.to_byte_array());
            self.commitments.insert(commitment, index);
        }
        self.root_history.insert(self.digest());
    }

    fn contains(&self, commitment: &Commitment) -> bool {
        self.commitments.contains_key(commitment)
    }

    /// Initializes an empty `CommitmentSet` with a given capacity.
    /// If the capacity is not a power_of_two, then capacity is taken
    /// to be the next power_of_two.
    pub(crate) fn with_capacity(capacity: usize) -> CommitmentSet {
        Self {
            merkle_tree: MerkleTree::with_capacity(capacity),
            commitments: HashMap::new(),
            root_history: HashSet::new(),
        }
    }
}

type NullifierSet = HashSet<Nullifier>;

pub struct V02State {
    public_state: HashMap<Address, Account>,
    private_state: (CommitmentSet, NullifierSet),
    programs: HashMap<ProgramId, Program>,
}

impl V02State {
    pub fn new_with_genesis_accounts(
        initial_data: &[(Address, u128)],
        initial_commitments: &[nssa_core::Commitment],
    ) -> Self {
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

        let mut private_state = CommitmentSet::with_capacity(32);
        private_state.extend(&[DUMMY_COMMITMENT]);
        private_state.extend(initial_commitments);

        let mut this = Self {
            public_state,
            private_state: (private_state, NullifierSet::new()),
            programs: HashMap::new(),
        };

        this.insert_program(Program::authenticated_transfer_program());
        this.insert_program(Program::token());

        this
    }

    pub(crate) fn insert_program(&mut self, program: Program) {
        self.programs.insert(program.id(), program);
    }

    pub fn transition_from_public_transaction(
        &mut self,
        tx: &PublicTransaction,
    ) -> Result<(), NssaError> {
        let state_diff = tx.validate_and_produce_public_state_diff(self)?;

        for (address, post) in state_diff.into_iter() {
            let current_account = self.get_account_by_address_mut(address);

            *current_account = post;
            // The invoked program claims the accounts with default program id.
            if current_account.program_owner == DEFAULT_PROGRAM_ID {
                current_account.program_owner = tx.message().program_id;
            }
        }

        for address in tx.signer_addresses() {
            let current_account = self.get_account_by_address_mut(address);
            current_account.nonce += 1;
        }

        Ok(())
    }

    pub fn transition_from_privacy_preserving_transaction(
        &mut self,
        tx: &PrivacyPreservingTransaction,
    ) -> Result<(), NssaError> {
        // 1. Verify the transaction satisfies acceptance criteria
        let public_state_diff = tx.validate_and_produce_public_state_diff(self)?;

        let message = tx.message();

        // 2. Add new commitments
        self.private_state.0.extend(&message.new_commitments);

        // 3. Add new nullifiers
        let new_nullifiers = message
            .new_nullifiers
            .iter()
            .cloned()
            .map(|(nullifier, _)| nullifier)
            .collect::<Vec<Nullifier>>();
        self.private_state.1.extend(new_nullifiers);

        // 4. Update public accounts
        for (address, post) in public_state_diff.into_iter() {
            let current_account = self.get_account_by_address_mut(address);
            *current_account = post;
        }

        Ok(())
    }

    pub fn transition_from_program_deployment_transaction(
        &mut self,
        tx: &ProgramDeploymentTransaction,
    ) -> Result<(), NssaError> {
        let program = tx.validate_and_produce_public_state_diff(self)?;
        self.insert_program(program);
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

    pub fn get_proof_for_commitment(&self, commitment: &Commitment) -> Option<MembershipProof> {
        self.private_state.0.get_proof_for(commitment)
    }

    pub(crate) fn programs(&self) -> &HashMap<ProgramId, Program> {
        &self.programs
    }

    pub fn commitment_set_digest(&self) -> CommitmentSetDigest {
        self.private_state.0.digest()
    }

    pub(crate) fn check_commitments_are_new(
        &self,
        new_commitments: &[Commitment],
    ) -> Result<(), NssaError> {
        for commitment in new_commitments.iter() {
            if self.private_state.0.contains(commitment) {
                return Err(NssaError::InvalidInput(
                    "Commitment already seen".to_string(),
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn check_nullifiers_are_valid(
        &self,
        new_nullifiers: &[(Nullifier, CommitmentSetDigest)],
    ) -> Result<(), NssaError> {
        for (nullifier, digest) in new_nullifiers.iter() {
            if self.private_state.1.contains(nullifier) {
                return Err(NssaError::InvalidInput(
                    "Nullifier already seen".to_string(),
                ));
            }
            if !self.private_state.0.root_history.contains(digest) {
                return Err(NssaError::InvalidInput(
                    "Unrecognized commitment set digest".to_string(),
                ));
            }
        }
        Ok(())
    }
}

// TODO: Testnet only. Refactor to prevent compilation on mainnet.
impl V02State {
    pub fn add_pinata_program(&mut self, address: Address) {
        self.insert_program(Program::pinata());

        self.public_state.insert(
            address,
            Account {
                program_owner: Program::pinata().id(),
                balance: 1500,
                // Difficulty: 3
                data: vec![3; 33],
                nonce: 0,
            },
        );
    }
}

#[cfg(test)]
pub mod tests {

    use std::collections::HashMap;

    use crate::{
        Address, PublicKey, PublicTransaction, V02State,
        error::NssaError,
        execute_and_prove,
        privacy_preserving_transaction::{
            PrivacyPreservingTransaction, circuit, message::Message, witness_set::WitnessSet,
        },
        program::Program,
        public_transaction,
        signature::PrivateKey,
    };

    use nssa_core::{
        Commitment, Nullifier, NullifierPublicKey, NullifierSecretKey, SharedSecretKey,
        account::{Account, AccountId, AccountWithMetadata, Nonce},
        encryption::{EphemeralPublicKey, IncomingViewingPublicKey, Scalar},
    };

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
        let authenticated_transfers_program = Program::authenticated_transfer_program();
        let expected_public_state = {
            let mut this = HashMap::new();
            this.insert(
                addr1,
                Account {
                    balance: 100,
                    program_owner: authenticated_transfers_program.id(),
                    ..Account::default()
                },
            );
            this.insert(
                addr2,
                Account {
                    balance: 151,
                    program_owner: authenticated_transfers_program.id(),
                    ..Account::default()
                },
            );
            this
        };
        let expected_builtin_programs = {
            let mut this = HashMap::new();
            this.insert(
                authenticated_transfers_program.id(),
                authenticated_transfers_program,
            );
            this.insert(Program::token().id(), Program::token());
            this
        };

        let state = V02State::new_with_genesis_accounts(&initial_data, &[]);

        assert_eq!(state.public_state, expected_public_state);
        assert_eq!(state.programs, expected_builtin_programs);
    }

    #[test]
    fn test_insert_program() {
        let mut state = V02State::new_with_genesis_accounts(&[], &[]);
        let program_to_insert = Program::simple_balance_transfer();
        let program_id = program_to_insert.id();
        assert!(!state.programs.contains_key(&program_id));

        state.insert_program(program_to_insert);

        assert!(state.programs.contains_key(&program_id));
    }

    #[test]
    fn test_get_account_by_address_non_default_account() {
        let key = PrivateKey::try_new([1; 32]).unwrap();
        let addr = Address::from(&PublicKey::new_from_private_key(&key));
        let initial_data = [(addr, 100u128)];
        let state = V02State::new_with_genesis_accounts(&initial_data, &[]);
        let expected_account = state.public_state.get(&addr).unwrap();

        let account = state.get_account_by_address(&addr);

        assert_eq!(&account, expected_account);
    }

    #[test]
    fn test_get_account_by_address_default_account() {
        let addr2 = Address::new([0; 32]);
        let state = V02State::new_with_genesis_accounts(&[], &[]);
        let expected_account = Account::default();

        let account = state.get_account_by_address(&addr2);

        assert_eq!(account, expected_account);
    }

    #[test]
    fn test_builtin_programs_getter() {
        let state = V02State::new_with_genesis_accounts(&[], &[]);

        let builtin_programs = state.programs();

        assert_eq!(builtin_programs, &state.programs);
    }

    #[test]
    fn transition_from_authenticated_transfer_program_invocation_default_account_destination() {
        let key = PrivateKey::try_new([1; 32]).unwrap();
        let address = Address::from(&PublicKey::new_from_private_key(&key));
        let initial_data = [(address, 100)];
        let mut state = V02State::new_with_genesis_accounts(&initial_data, &[]);
        let from = address;
        let to = Address::new([2; 32]);
        assert_eq!(state.get_account_by_address(&to), Account::default());
        let balance_to_move = 5;

        let tx = transfer_transaction(from, key, 0, to, balance_to_move);
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
        let mut state = V02State::new_with_genesis_accounts(&initial_data, &[]);
        let from = address;
        let from_key = key;
        let to = Address::new([2; 32]);
        let balance_to_move = 101;
        assert!(state.get_account_by_address(&from).balance < balance_to_move);

        let tx = transfer_transaction(from, from_key, 0, to, balance_to_move);
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
        let mut state = V02State::new_with_genesis_accounts(&initial_data, &[]);
        let from = address2;
        let from_key = key2;
        let to = address1;
        assert_ne!(state.get_account_by_address(&to), Account::default());
        let balance_to_move = 8;

        let tx = transfer_transaction(from, from_key, 0, to, balance_to_move);
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
        let mut state = V02State::new_with_genesis_accounts(&initial_data, &[]);
        let address3 = Address::new([3; 32]);
        let balance_to_move = 5;

        let tx = transfer_transaction(address1, key1, 0, address2, balance_to_move);
        state.transition_from_public_transaction(&tx).unwrap();
        let balance_to_move = 3;
        let tx = transfer_transaction(address2, key2, 0, address3, balance_to_move);
        state.transition_from_public_transaction(&tx).unwrap();

        assert_eq!(state.get_account_by_address(&address1).balance, 95);
        assert_eq!(state.get_account_by_address(&address2).balance, 2);
        assert_eq!(state.get_account_by_address(&address3).balance, 3);
        assert_eq!(state.get_account_by_address(&address1).nonce, 1);
        assert_eq!(state.get_account_by_address(&address2).nonce, 1);
        assert_eq!(state.get_account_by_address(&address3).nonce, 0);
    }

    impl V02State {
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

        pub fn with_private_account(mut self, keys: &TestPrivateKeys, account: &Account) -> Self {
            let commitment = Commitment::new(&keys.npk(), account);
            self.private_state.0.extend(&[commitment]);
            self
        }
    }

    #[test]
    fn test_program_should_fail_if_modifies_nonces() {
        let initial_data = [(Address::new([1; 32]), 100)];
        let mut state =
            V02State::new_with_genesis_accounts(&initial_data, &[]).with_test_programs();
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
        let mut state =
            V02State::new_with_genesis_accounts(&initial_data, &[]).with_test_programs();
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
        let mut state =
            V02State::new_with_genesis_accounts(&initial_data, &[]).with_test_programs();
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
        let mut state =
            V02State::new_with_genesis_accounts(&initial_data, &[]).with_test_programs();
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
        let mut state = V02State::new_with_genesis_accounts(&initial_data, &[])
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
        let mut state = V02State::new_with_genesis_accounts(&initial_data, &[])
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
        let mut state = V02State::new_with_genesis_accounts(&initial_data, &[])
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
        let mut state =
            V02State::new_with_genesis_accounts(&initial_data, &[]).with_test_programs();
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
        let mut state = V02State::new_with_genesis_accounts(&initial_data, &[])
            .with_test_programs()
            .with_non_default_accounts_but_default_program_owners();
        let address = Address::new([255; 32]);
        let program_id = Program::data_changer().id();

        assert_ne!(state.get_account_by_address(&address), Account::default());
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
        let mut state =
            V02State::new_with_genesis_accounts(&initial_data, &[]).with_test_programs();
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
        let mut state = V02State::new_with_genesis_accounts(&initial_data, &[])
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

    pub struct TestPublicKeys {
        pub signing_key: PrivateKey,
    }

    impl TestPublicKeys {
        pub fn address(&self) -> Address {
            Address::from(&PublicKey::new_from_private_key(&self.signing_key))
        }
    }

    fn test_public_account_keys_1() -> TestPublicKeys {
        TestPublicKeys {
            signing_key: PrivateKey::try_new([37; 32]).unwrap(),
        }
    }

    pub struct TestPrivateKeys {
        pub nsk: NullifierSecretKey,
        pub isk: Scalar,
    }

    impl TestPrivateKeys {
        pub fn npk(&self) -> NullifierPublicKey {
            NullifierPublicKey::from(&self.nsk)
        }

        pub fn ivk(&self) -> IncomingViewingPublicKey {
            IncomingViewingPublicKey::from_scalar(self.isk)
        }
    }

    pub fn test_private_account_keys_1() -> TestPrivateKeys {
        TestPrivateKeys {
            nsk: [13; 32],
            isk: [31; 32],
        }
    }

    pub fn test_private_account_keys_2() -> TestPrivateKeys {
        TestPrivateKeys {
            nsk: [38; 32],
            isk: [83; 32],
        }
    }

    fn shielded_balance_transfer_for_tests(
        sender_keys: &TestPublicKeys,
        recipient_keys: &TestPrivateKeys,
        balance_to_move: u128,
        state: &V02State,
    ) -> PrivacyPreservingTransaction {
        let sender = AccountWithMetadata::new(
            state.get_account_by_address(&sender_keys.address()),
            true,
            sender_keys.address(),
        );

        let sender_nonce = sender.account.nonce;

        let recipient = AccountWithMetadata::new(Account::default(), false, &recipient_keys.npk());

        let esk = [3; 32];
        let shared_secret = SharedSecretKey::new(&esk, &recipient_keys.ivk());
        let epk = EphemeralPublicKey::from_scalar(esk);

        let (output, proof) = circuit::execute_and_prove(
            &[sender, recipient],
            &Program::serialize_instruction(balance_to_move).unwrap(),
            &[0, 2],
            &[0xdeadbeef],
            &[(recipient_keys.npk(), shared_secret)],
            &[],
            &Program::authenticated_transfer_program(),
        )
        .unwrap();

        let message = Message::try_from_circuit_output(
            vec![sender_keys.address()],
            vec![sender_nonce],
            vec![(recipient_keys.npk(), recipient_keys.ivk(), epk)],
            output,
        )
        .unwrap();

        let witness_set = WitnessSet::for_message(&message, proof, &[&sender_keys.signing_key]);
        PrivacyPreservingTransaction::new(message, witness_set)
    }

    fn private_balance_transfer_for_tests(
        sender_keys: &TestPrivateKeys,
        sender_private_account: &Account,
        recipient_keys: &TestPrivateKeys,
        balance_to_move: u128,
        new_nonces: [Nonce; 2],
        state: &V02State,
    ) -> PrivacyPreservingTransaction {
        let program = Program::authenticated_transfer_program();
        let sender_commitment = Commitment::new(&sender_keys.npk(), sender_private_account);
        let sender_pre =
            AccountWithMetadata::new(sender_private_account.clone(), true, &sender_keys.npk());
        let recipient_pre =
            AccountWithMetadata::new(Account::default(), false, &recipient_keys.npk());

        let esk_1 = [3; 32];
        let shared_secret_1 = SharedSecretKey::new(&esk_1, &sender_keys.ivk());
        let epk_1 = EphemeralPublicKey::from_scalar(esk_1);

        let esk_2 = [3; 32];
        let shared_secret_2 = SharedSecretKey::new(&esk_2, &recipient_keys.ivk());
        let epk_2 = EphemeralPublicKey::from_scalar(esk_2);

        let (output, proof) = circuit::execute_and_prove(
            &[sender_pre, recipient_pre],
            &Program::serialize_instruction(balance_to_move).unwrap(),
            &[1, 2],
            &new_nonces,
            &[
                (sender_keys.npk(), shared_secret_1),
                (recipient_keys.npk(), shared_secret_2),
            ],
            &[(
                sender_keys.nsk,
                state.get_proof_for_commitment(&sender_commitment).unwrap(),
            )],
            &program,
        )
        .unwrap();

        let message = Message::try_from_circuit_output(
            vec![],
            vec![],
            vec![
                (sender_keys.npk(), sender_keys.ivk(), epk_1),
                (recipient_keys.npk(), recipient_keys.ivk(), epk_2),
            ],
            output,
        )
        .unwrap();

        let witness_set = WitnessSet::for_message(&message, proof, &[]);

        PrivacyPreservingTransaction::new(message, witness_set)
    }

    fn deshielded_balance_transfer_for_tests(
        sender_keys: &TestPrivateKeys,
        sender_private_account: &Account,
        recipient_address: &Address,
        balance_to_move: u128,
        new_nonce: Nonce,
        state: &V02State,
    ) -> PrivacyPreservingTransaction {
        let program = Program::authenticated_transfer_program();
        let sender_commitment = Commitment::new(&sender_keys.npk(), sender_private_account);
        let sender_pre =
            AccountWithMetadata::new(sender_private_account.clone(), true, &sender_keys.npk());
        let recipient_pre = AccountWithMetadata::new(
            state.get_account_by_address(recipient_address),
            false,
            *recipient_address,
        );

        let esk = [3; 32];
        let shared_secret = SharedSecretKey::new(&esk, &sender_keys.ivk());
        let epk = EphemeralPublicKey::from_scalar(esk);

        let (output, proof) = circuit::execute_and_prove(
            &[sender_pre, recipient_pre],
            &Program::serialize_instruction(balance_to_move).unwrap(),
            &[1, 0],
            &[new_nonce],
            &[(sender_keys.npk(), shared_secret)],
            &[(
                sender_keys.nsk,
                state.get_proof_for_commitment(&sender_commitment).unwrap(),
            )],
            &program,
        )
        .unwrap();

        let message = Message::try_from_circuit_output(
            vec![*recipient_address],
            vec![],
            vec![(sender_keys.npk(), sender_keys.ivk(), epk)],
            output,
        )
        .unwrap();

        let witness_set = WitnessSet::for_message(&message, proof, &[]);

        PrivacyPreservingTransaction::new(message, witness_set)
    }

    #[test]
    fn test_transition_from_privacy_preserving_transaction_shielded() {
        let sender_keys = test_public_account_keys_1();
        let recipient_keys = test_private_account_keys_1();

        let mut state = V02State::new_with_genesis_accounts(&[(sender_keys.address(), 200)], &[]);

        let balance_to_move = 37;

        let tx = shielded_balance_transfer_for_tests(
            &sender_keys,
            &recipient_keys,
            balance_to_move,
            &state,
        );

        let expected_sender_post = {
            let mut this = state.get_account_by_address(&sender_keys.address());
            this.balance -= balance_to_move;
            this.nonce += 1;
            this
        };

        let [expected_new_commitment] = tx.message().new_commitments.clone().try_into().unwrap();
        assert!(!state.private_state.0.contains(&expected_new_commitment));

        state
            .transition_from_privacy_preserving_transaction(&tx)
            .unwrap();

        let sender_post = state.get_account_by_address(&sender_keys.address());
        assert_eq!(sender_post, expected_sender_post);
        assert!(state.private_state.0.contains(&expected_new_commitment));

        assert_eq!(
            state.get_account_by_address(&sender_keys.address()).balance,
            200 - balance_to_move
        );
    }

    #[test]
    fn test_transition_from_privacy_preserving_transaction_private() {
        let sender_keys = test_private_account_keys_1();
        let sender_private_account = Account {
            program_owner: Program::authenticated_transfer_program().id(),
            balance: 100,
            nonce: 0xdeadbeef,
            data: vec![],
        };
        let recipient_keys = test_private_account_keys_2();

        let mut state = V02State::new_with_genesis_accounts(&[], &[])
            .with_private_account(&sender_keys, &sender_private_account);

        let balance_to_move = 37;

        let tx = private_balance_transfer_for_tests(
            &sender_keys,
            &sender_private_account,
            &recipient_keys,
            balance_to_move,
            [0xcafecafe, 0xfecafeca],
            &state,
        );

        let expected_new_commitment_1 = Commitment::new(
            &sender_keys.npk(),
            &Account {
                program_owner: Program::authenticated_transfer_program().id(),
                nonce: 0xcafecafe,
                balance: sender_private_account.balance - balance_to_move,
                data: vec![],
            },
        );

        let sender_pre_commitment = Commitment::new(&sender_keys.npk(), &sender_private_account);
        let expected_new_nullifier =
            Nullifier::for_account_update(&sender_pre_commitment, &sender_keys.nsk);

        let expected_new_commitment_2 = Commitment::new(
            &recipient_keys.npk(),
            &Account {
                program_owner: Program::authenticated_transfer_program().id(),
                nonce: 0xfecafeca,
                balance: balance_to_move,
                ..Account::default()
            },
        );

        let previous_public_state = state.public_state.clone();
        assert!(state.private_state.0.contains(&sender_pre_commitment));
        assert!(!state.private_state.0.contains(&expected_new_commitment_1));
        assert!(!state.private_state.0.contains(&expected_new_commitment_2));
        assert!(!state.private_state.1.contains(&expected_new_nullifier));

        state
            .transition_from_privacy_preserving_transaction(&tx)
            .unwrap();

        assert_eq!(state.public_state, previous_public_state);
        assert!(state.private_state.0.contains(&sender_pre_commitment));
        assert!(state.private_state.0.contains(&expected_new_commitment_1));
        assert!(state.private_state.0.contains(&expected_new_commitment_2));
        assert!(state.private_state.1.contains(&expected_new_nullifier));
    }

    #[test]
    fn test_transition_from_privacy_preserving_transaction_deshielded() {
        let sender_keys = test_private_account_keys_1();
        let sender_private_account = Account {
            program_owner: Program::authenticated_transfer_program().id(),
            balance: 100,
            nonce: 0xdeadbeef,
            data: vec![],
        };
        let recipient_keys = test_public_account_keys_1();
        let recipient_initial_balance = 400;
        let mut state = V02State::new_with_genesis_accounts(
            &[(recipient_keys.address(), recipient_initial_balance)],
            &[],
        )
        .with_private_account(&sender_keys, &sender_private_account);

        let balance_to_move = 37;

        let expected_recipient_post = {
            let mut this = state.get_account_by_address(&recipient_keys.address());
            this.balance += balance_to_move;
            this
        };

        let tx = deshielded_balance_transfer_for_tests(
            &sender_keys,
            &sender_private_account,
            &recipient_keys.address(),
            balance_to_move,
            0xcafecafe,
            &state,
        );

        let expected_new_commitment = Commitment::new(
            &sender_keys.npk(),
            &Account {
                program_owner: Program::authenticated_transfer_program().id(),
                nonce: 0xcafecafe,
                balance: sender_private_account.balance - balance_to_move,
                data: vec![],
            },
        );

        let sender_pre_commitment = Commitment::new(&sender_keys.npk(), &sender_private_account);
        let expected_new_nullifier =
            Nullifier::for_account_update(&sender_pre_commitment, &sender_keys.nsk);

        assert!(state.private_state.0.contains(&sender_pre_commitment));
        assert!(!state.private_state.0.contains(&expected_new_commitment));
        assert!(!state.private_state.1.contains(&expected_new_nullifier));

        state
            .transition_from_privacy_preserving_transaction(&tx)
            .unwrap();

        let recipient_post = state.get_account_by_address(&recipient_keys.address());
        assert_eq!(recipient_post, expected_recipient_post);
        assert!(state.private_state.0.contains(&sender_pre_commitment));
        assert!(state.private_state.0.contains(&expected_new_commitment));
        assert!(state.private_state.1.contains(&expected_new_nullifier));
        assert_eq!(
            state
                .get_account_by_address(&recipient_keys.address())
                .balance,
            recipient_initial_balance + balance_to_move
        );
    }

    #[test]
    fn test_burner_program_should_fail_in_privacy_preserving_circuit() {
        let program = Program::burner();
        let public_account = AccountWithMetadata::new(
            Account {
                program_owner: program.id(),
                balance: 100,
                ..Account::default()
            },
            true,
            AccountId::new([0; 32]),
        );

        let result = execute_and_prove(
            &[public_account],
            &Program::serialize_instruction(10u128).unwrap(),
            &[0],
            &[],
            &[],
            &[],
            &program,
        );

        assert!(matches!(result, Err(NssaError::CircuitProvingError(_))));
    }

    #[test]
    fn test_minter_program_should_fail_in_privacy_preserving_circuit() {
        let program = Program::minter();
        let public_account = AccountWithMetadata::new(
            Account {
                program_owner: program.id(),
                balance: 0,
                ..Account::default()
            },
            true,
            AccountId::new([0; 32]),
        );

        let result = execute_and_prove(
            &[public_account],
            &Program::serialize_instruction(10u128).unwrap(),
            &[0],
            &[],
            &[],
            &[],
            &program,
        );

        assert!(matches!(result, Err(NssaError::CircuitProvingError(_))));
    }

    #[test]
    fn test_nonce_changer_program_should_fail_in_privacy_preserving_circuit() {
        let program = Program::nonce_changer_program();
        let public_account = AccountWithMetadata::new(
            Account {
                program_owner: program.id(),
                balance: 0,
                ..Account::default()
            },
            true,
            AccountId::new([0; 32]),
        );

        let result = execute_and_prove(
            &[public_account],
            &Program::serialize_instruction(()).unwrap(),
            &[0],
            &[],
            &[],
            &[],
            &program,
        );

        assert!(matches!(result, Err(NssaError::CircuitProvingError(_))));
    }

    #[test]
    fn test_data_changer_program_should_fail_for_non_owned_account_in_privacy_preserving_circuit() {
        let program = Program::data_changer();
        let public_account = AccountWithMetadata::new(
            Account {
                program_owner: [0, 1, 2, 3, 4, 5, 6, 7],
                balance: 0,
                ..Account::default()
            },
            true,
            AccountId::new([0; 32]),
        );

        let result = execute_and_prove(
            &[public_account],
            &Program::serialize_instruction(()).unwrap(),
            &[0],
            &[],
            &[],
            &[],
            &program,
        );

        assert!(matches!(result, Err(NssaError::CircuitProvingError(_))));
    }

    #[test]
    fn test_extra_output_program_should_fail_in_privacy_preserving_circuit() {
        let program = Program::extra_output_program();
        let public_account = AccountWithMetadata::new(
            Account {
                program_owner: program.id(),
                balance: 0,
                ..Account::default()
            },
            true,
            AccountId::new([0; 32]),
        );

        let result = execute_and_prove(
            &[public_account],
            &Program::serialize_instruction(()).unwrap(),
            &[0],
            &[],
            &[],
            &[],
            &program,
        );

        assert!(matches!(result, Err(NssaError::CircuitProvingError(_))));
    }

    #[test]
    fn test_missing_output_program_should_fail_in_privacy_preserving_circuit() {
        let program = Program::missing_output_program();
        let public_account_1 = AccountWithMetadata::new(
            Account {
                program_owner: program.id(),
                balance: 0,
                ..Account::default()
            },
            true,
            AccountId::new([0; 32]),
        );
        let public_account_2 = AccountWithMetadata::new(
            Account {
                program_owner: program.id(),
                balance: 0,
                ..Account::default()
            },
            true,
            AccountId::new([1; 32]),
        );

        let result = execute_and_prove(
            &[public_account_1, public_account_2],
            &Program::serialize_instruction(()).unwrap(),
            &[0, 0],
            &[],
            &[],
            &[],
            &program,
        );

        assert!(matches!(result, Err(NssaError::CircuitProvingError(_))));
    }

    #[test]
    fn test_program_owner_changer_should_fail_in_privacy_preserving_circuit() {
        let program = Program::program_owner_changer();
        let public_account = AccountWithMetadata::new(
            Account {
                program_owner: program.id(),
                balance: 0,
                ..Account::default()
            },
            true,
            AccountId::new([0; 32]),
        );

        let result = execute_and_prove(
            &[public_account],
            &Program::serialize_instruction(()).unwrap(),
            &[0],
            &[],
            &[],
            &[],
            &program,
        );

        assert!(matches!(result, Err(NssaError::CircuitProvingError(_))));
    }

    #[test]
    fn test_transfer_from_non_owned_account_should_fail_in_privacy_preserving_circuit() {
        let program = Program::simple_balance_transfer();
        let public_account_1 = AccountWithMetadata::new(
            Account {
                program_owner: [0, 1, 2, 3, 4, 5, 6, 7],
                balance: 100,
                ..Account::default()
            },
            true,
            AccountId::new([0; 32]),
        );
        let public_account_2 = AccountWithMetadata::new(
            Account {
                program_owner: program.id(),
                balance: 0,
                ..Account::default()
            },
            true,
            AccountId::new([1; 32]),
        );

        let result = execute_and_prove(
            &[public_account_1, public_account_2],
            &Program::serialize_instruction(10u128).unwrap(),
            &[0, 0],
            &[],
            &[],
            &[],
            &program,
        );

        assert!(matches!(result, Err(NssaError::CircuitProvingError(_))));
    }

    #[test]
    fn test_circuit_fails_if_visibility_masks_have_incorrect_lenght() {
        let program = Program::simple_balance_transfer();
        let public_account_1 = AccountWithMetadata::new(
            Account {
                program_owner: program.id(),
                balance: 100,
                ..Account::default()
            },
            true,
            AccountId::new([0; 32]),
        );
        let public_account_2 = AccountWithMetadata::new(
            Account {
                program_owner: program.id(),
                balance: 0,
                ..Account::default()
            },
            true,
            AccountId::new([1; 32]),
        );

        // Setting only one visibility mask for a circuit execution with two pre_state accounts.
        let visibility_mask = [0];
        let result = execute_and_prove(
            &[public_account_1, public_account_2],
            &Program::serialize_instruction(10u128).unwrap(),
            &visibility_mask,
            &[],
            &[],
            &[],
            &program,
        );

        assert!(matches!(result, Err(NssaError::CircuitProvingError(_))));
    }

    #[test]
    fn test_circuit_fails_if_insufficient_nonces_are_provided() {
        let program = Program::simple_balance_transfer();
        let sender_keys = test_private_account_keys_1();
        let recipient_keys = test_private_account_keys_2();
        let private_account_1 = AccountWithMetadata::new(
            Account {
                program_owner: program.id(),
                balance: 100,
                ..Account::default()
            },
            true,
            &sender_keys.npk(),
        );
        let private_account_2 =
            AccountWithMetadata::new(Account::default(), false, &recipient_keys.npk());

        // Setting only one nonce for an execution with two private accounts.
        let private_account_nonces = [0xdeadbeef1];
        let result = execute_and_prove(
            &[private_account_1, private_account_2],
            &Program::serialize_instruction(10u128).unwrap(),
            &[1, 2],
            &private_account_nonces,
            &[
                (
                    sender_keys.npk(),
                    SharedSecretKey::new(&[55; 32], &sender_keys.ivk()),
                ),
                (
                    recipient_keys.npk(),
                    SharedSecretKey::new(&[56; 32], &recipient_keys.ivk()),
                ),
            ],
            &[(sender_keys.nsk, (0, vec![]))],
            &program,
        );

        assert!(matches!(result, Err(NssaError::CircuitProvingError(_))));
    }

    #[test]
    fn test_circuit_fails_if_insufficient_keys_are_provided() {
        let program = Program::simple_balance_transfer();
        let sender_keys = test_private_account_keys_1();
        let private_account_1 = AccountWithMetadata::new(
            Account {
                program_owner: program.id(),
                balance: 100,
                ..Account::default()
            },
            true,
            &sender_keys.npk(),
        );
        let private_account_2 =
            AccountWithMetadata::new(Account::default(), false, AccountId::new([1; 32]));

        // Setting only one key for an execution with two private accounts.
        let private_account_keys = [(
            sender_keys.npk(),
            SharedSecretKey::new(&[55; 32], &sender_keys.ivk()),
        )];
        let result = execute_and_prove(
            &[private_account_1, private_account_2],
            &Program::serialize_instruction(10u128).unwrap(),
            &[1, 2],
            &[0xdeadbeef1, 0xdeadbeef2],
            &private_account_keys,
            &[(sender_keys.nsk, (0, vec![]))],
            &program,
        );

        assert!(matches!(result, Err(NssaError::CircuitProvingError(_))));
    }

    #[test]
    fn test_circuit_fails_if_insufficient_auth_keys_are_provided() {
        let program = Program::simple_balance_transfer();
        let sender_keys = test_private_account_keys_1();
        let recipient_keys = test_private_account_keys_2();
        let private_account_1 = AccountWithMetadata::new(
            Account {
                program_owner: program.id(),
                balance: 100,
                ..Account::default()
            },
            true,
            &sender_keys.npk(),
        );
        let private_account_2 =
            AccountWithMetadata::new(Account::default(), false, &recipient_keys.npk());

        // Setting no auth key for an execution with one non default private accounts.
        let private_account_auth = [];
        let result = execute_and_prove(
            &[private_account_1, private_account_2],
            &Program::serialize_instruction(10u128).unwrap(),
            &[1, 2],
            &[0xdeadbeef1, 0xdeadbeef2],
            &[
                (
                    sender_keys.npk(),
                    SharedSecretKey::new(&[55; 32], &sender_keys.ivk()),
                ),
                (
                    recipient_keys.npk(),
                    SharedSecretKey::new(&[56; 32], &recipient_keys.ivk()),
                ),
            ],
            &private_account_auth,
            &program,
        );

        assert!(matches!(result, Err(NssaError::CircuitProvingError(_))));
    }

    #[test]
    fn test_circuit_fails_if_invalid_auth_keys_are_provided() {
        let program = Program::simple_balance_transfer();
        let sender_keys = test_private_account_keys_1();
        let recipient_keys = test_private_account_keys_2();
        let private_account_1 = AccountWithMetadata::new(
            Account {
                program_owner: program.id(),
                balance: 100,
                ..Account::default()
            },
            true,
            &sender_keys.npk(),
        );
        let private_account_2 =
            AccountWithMetadata::new(Account::default(), false, &recipient_keys.npk());

        let private_account_keys = [
            // First private account is the sender
            (
                sender_keys.npk(),
                SharedSecretKey::new(&[55; 32], &sender_keys.ivk()),
            ),
            // Second private account is the recipient
            (
                recipient_keys.npk(),
                SharedSecretKey::new(&[56; 32], &recipient_keys.ivk()),
            ),
        ];
        let private_account_auth = [
            // Setting the recipient key to authorize the sender.
            // This should be set to the sender private account in
            // a normal circumstance. The recipient can't authorize this.
            (recipient_keys.nsk, (0, vec![])),
        ];
        let result = execute_and_prove(
            &[private_account_1, private_account_2],
            &Program::serialize_instruction(10u128).unwrap(),
            &[1, 2],
            &[0xdeadbeef1, 0xdeadbeef2],
            &private_account_keys,
            &private_account_auth,
            &program,
        );

        assert!(matches!(result, Err(NssaError::CircuitProvingError(_))));
    }

    #[test]
    fn test_circuit_should_fail_if_new_private_account_with_non_default_balance_is_provided() {
        let program = Program::simple_balance_transfer();
        let sender_keys = test_private_account_keys_1();
        let recipient_keys = test_private_account_keys_2();
        let private_account_1 = AccountWithMetadata::new(
            Account {
                program_owner: program.id(),
                balance: 100,
                ..Account::default()
            },
            true,
            &sender_keys.npk(),
        );
        let private_account_2 = AccountWithMetadata::new(
            Account {
                // Non default balance
                balance: 1,
                ..Account::default()
            },
            false,
            &recipient_keys.npk(),
        );

        let result = execute_and_prove(
            &[private_account_1, private_account_2],
            &Program::serialize_instruction(10u128).unwrap(),
            &[1, 2],
            &[0xdeadbeef1, 0xdeadbeef2],
            &[
                (
                    sender_keys.npk(),
                    SharedSecretKey::new(&[55; 32], &sender_keys.ivk()),
                ),
                (
                    recipient_keys.npk(),
                    SharedSecretKey::new(&[56; 32], &recipient_keys.ivk()),
                ),
            ],
            &[(sender_keys.nsk, (0, vec![]))],
            &program,
        );

        assert!(matches!(result, Err(NssaError::CircuitProvingError(_))));
    }

    #[test]
    fn test_circuit_should_fail_if_new_private_account_with_non_default_program_owner_is_provided()
    {
        let program = Program::simple_balance_transfer();
        let sender_keys = test_private_account_keys_1();
        let recipient_keys = test_private_account_keys_2();
        let private_account_1 = AccountWithMetadata::new(
            Account {
                program_owner: program.id(),
                balance: 100,
                ..Account::default()
            },
            true,
            &sender_keys.npk(),
        );
        let private_account_2 = AccountWithMetadata::new(
            Account {
                // Non default program_owner
                program_owner: [0, 1, 2, 3, 4, 5, 6, 7],
                ..Account::default()
            },
            false,
            &recipient_keys.npk(),
        );

        let result = execute_and_prove(
            &[private_account_1, private_account_2],
            &Program::serialize_instruction(10u128).unwrap(),
            &[1, 2],
            &[0xdeadbeef1, 0xdeadbeef2],
            &[
                (
                    sender_keys.npk(),
                    SharedSecretKey::new(&[55; 32], &sender_keys.ivk()),
                ),
                (
                    recipient_keys.npk(),
                    SharedSecretKey::new(&[56; 32], &recipient_keys.ivk()),
                ),
            ],
            &[(sender_keys.nsk, (0, vec![]))],
            &program,
        );

        assert!(matches!(result, Err(NssaError::CircuitProvingError(_))));
    }

    #[test]
    fn test_circuit_should_fail_if_new_private_account_with_non_default_data_is_provided() {
        let program = Program::simple_balance_transfer();
        let sender_keys = test_private_account_keys_1();
        let recipient_keys = test_private_account_keys_2();
        let private_account_1 = AccountWithMetadata::new(
            Account {
                program_owner: program.id(),
                balance: 100,
                ..Account::default()
            },
            true,
            &sender_keys.npk(),
        );
        let private_account_2 = AccountWithMetadata::new(
            Account {
                // Non default data
                data: b"hola mundo".to_vec(),
                ..Account::default()
            },
            false,
            &recipient_keys.npk(),
        );

        let result = execute_and_prove(
            &[private_account_1, private_account_2],
            &Program::serialize_instruction(10u128).unwrap(),
            &[1, 2],
            &[0xdeadbeef1, 0xdeadbeef2],
            &[
                (
                    sender_keys.npk(),
                    SharedSecretKey::new(&[55; 32], &sender_keys.ivk()),
                ),
                (
                    recipient_keys.npk(),
                    SharedSecretKey::new(&[56; 32], &recipient_keys.ivk()),
                ),
            ],
            &[(sender_keys.nsk, (0, vec![]))],
            &program,
        );

        assert!(matches!(result, Err(NssaError::CircuitProvingError(_))));
    }

    #[test]
    fn test_circuit_should_fail_if_new_private_account_with_non_default_nonce_is_provided() {
        let program = Program::simple_balance_transfer();
        let sender_keys = test_private_account_keys_1();
        let recipient_keys = test_private_account_keys_2();
        let private_account_1 = AccountWithMetadata::new(
            Account {
                program_owner: program.id(),
                balance: 100,
                ..Account::default()
            },
            true,
            &sender_keys.npk(),
        );
        let private_account_2 = AccountWithMetadata::new(
            Account {
                // Non default nonce
                nonce: 0xdeadbeef,
                ..Account::default()
            },
            false,
            &recipient_keys.npk(),
        );

        let result = execute_and_prove(
            &[private_account_1, private_account_2],
            &Program::serialize_instruction(10u128).unwrap(),
            &[1, 2],
            &[0xdeadbeef1, 0xdeadbeef2],
            &[
                (
                    sender_keys.npk(),
                    SharedSecretKey::new(&[55; 32], &sender_keys.ivk()),
                ),
                (
                    recipient_keys.npk(),
                    SharedSecretKey::new(&[56; 32], &recipient_keys.ivk()),
                ),
            ],
            &[(sender_keys.nsk, (0, vec![]))],
            &program,
        );

        assert!(matches!(result, Err(NssaError::CircuitProvingError(_))));
    }

    #[test]
    fn test_circuit_should_fail_if_new_private_account_is_provided_with_default_values_but_marked_as_authorized()
     {
        let program = Program::simple_balance_transfer();
        let sender_keys = test_private_account_keys_1();
        let recipient_keys = test_private_account_keys_2();
        let private_account_1 = AccountWithMetadata::new(
            Account {
                program_owner: program.id(),
                balance: 100,
                ..Account::default()
            },
            true,
            &sender_keys.npk(),
        );
        let private_account_2 = AccountWithMetadata::new(
            Account::default(),
            // This should be set to false in normal circumstances
            true,
            &recipient_keys.npk(),
        );

        let result = execute_and_prove(
            &[private_account_1, private_account_2],
            &Program::serialize_instruction(10u128).unwrap(),
            &[1, 2],
            &[0xdeadbeef1, 0xdeadbeef2],
            &[
                (
                    sender_keys.npk(),
                    SharedSecretKey::new(&[55; 32], &sender_keys.ivk()),
                ),
                (
                    recipient_keys.npk(),
                    SharedSecretKey::new(&[56; 32], &recipient_keys.ivk()),
                ),
            ],
            &[(sender_keys.nsk, (0, vec![]))],
            &program,
        );

        assert!(matches!(result, Err(NssaError::CircuitProvingError(_))));
    }

    #[test]
    fn test_circuit_should_fail_with_invalid_visibility_mask_value() {
        let program = Program::simple_balance_transfer();
        let public_account_1 = AccountWithMetadata::new(
            Account {
                program_owner: program.id(),
                balance: 100,
                ..Account::default()
            },
            true,
            AccountId::new([0; 32]),
        );
        let public_account_2 =
            AccountWithMetadata::new(Account::default(), false, AccountId::new([1; 32]));

        let visibility_mask = [0, 3];
        let result = execute_and_prove(
            &[public_account_1, public_account_2],
            &Program::serialize_instruction(10u128).unwrap(),
            &visibility_mask,
            &[],
            &[],
            &[],
            &program,
        );

        assert!(matches!(result, Err(NssaError::CircuitProvingError(_))));
    }

    #[test]
    fn test_circuit_should_fail_with_too_many_nonces() {
        let program = Program::simple_balance_transfer();
        let sender_keys = test_private_account_keys_1();
        let recipient_keys = test_private_account_keys_2();
        let private_account_1 = AccountWithMetadata::new(
            Account {
                program_owner: program.id(),
                balance: 100,
                ..Account::default()
            },
            true,
            &sender_keys.npk(),
        );
        let private_account_2 =
            AccountWithMetadata::new(Account::default(), false, &recipient_keys.npk());

        // Setting three new private account nonces for a circuit execution with only two private
        // accounts.
        let private_account_nonces = [0xdeadbeef1, 0xdeadbeef2, 0xdeadbeef3];
        let result = execute_and_prove(
            &[private_account_1, private_account_2],
            &Program::serialize_instruction(10u128).unwrap(),
            &[1, 2],
            &private_account_nonces,
            &[
                (
                    sender_keys.npk(),
                    SharedSecretKey::new(&[55; 32], &sender_keys.ivk()),
                ),
                (
                    recipient_keys.npk(),
                    SharedSecretKey::new(&[56; 32], &recipient_keys.ivk()),
                ),
            ],
            &[(sender_keys.nsk, (0, vec![]))],
            &program,
        );

        assert!(matches!(result, Err(NssaError::CircuitProvingError(_))));
    }

    #[test]
    fn test_circuit_should_fail_with_too_many_private_account_keys() {
        let program = Program::simple_balance_transfer();
        let sender_keys = test_private_account_keys_1();
        let recipient_keys = test_private_account_keys_2();
        let private_account_1 = AccountWithMetadata::new(
            Account {
                program_owner: program.id(),
                balance: 100,
                ..Account::default()
            },
            true,
            &sender_keys.npk(),
        );
        let private_account_2 =
            AccountWithMetadata::new(Account::default(), false, &recipient_keys.npk());

        // Setting three private account keys for a circuit execution with only two private
        // accounts.
        let private_account_keys = [
            (
                sender_keys.npk(),
                SharedSecretKey::new(&[55; 32], &sender_keys.ivk()),
            ),
            (
                recipient_keys.npk(),
                SharedSecretKey::new(&[56; 32], &recipient_keys.ivk()),
            ),
            (
                sender_keys.npk(),
                SharedSecretKey::new(&[57; 32], &sender_keys.ivk()),
            ),
        ];
        let result = execute_and_prove(
            &[private_account_1, private_account_2],
            &Program::serialize_instruction(10u128).unwrap(),
            &[1, 2],
            &[0xdeadbeef1, 0xdeadbeef2],
            &private_account_keys,
            &[(sender_keys.nsk, (0, vec![]))],
            &program,
        );

        assert!(matches!(result, Err(NssaError::CircuitProvingError(_))));
    }

    #[test]
    fn test_circuit_should_fail_with_too_many_private_account_auth_keys() {
        let program = Program::simple_balance_transfer();
        let sender_keys = test_private_account_keys_1();
        let recipient_keys = test_private_account_keys_2();
        let private_account_1 = AccountWithMetadata::new(
            Account {
                program_owner: program.id(),
                balance: 100,
                ..Account::default()
            },
            true,
            &sender_keys.npk(),
        );
        let private_account_2 =
            AccountWithMetadata::new(Account::default(), false, &recipient_keys.npk());

        // Setting two private account keys for a circuit execution with only one non default
        // private account (visibility mask equal to 1 means that auth keys are expected).
        let visibility_mask = [1, 2];
        let private_account_auth = [
            (sender_keys.nsk, (0, vec![])),
            (recipient_keys.nsk, (1, vec![])),
        ];
        let result = execute_and_prove(
            &[private_account_1, private_account_2],
            &Program::serialize_instruction(10u128).unwrap(),
            &visibility_mask,
            &[0xdeadbeef1, 0xdeadbeef2],
            &[
                (
                    sender_keys.npk(),
                    SharedSecretKey::new(&[55; 32], &sender_keys.ivk()),
                ),
                (
                    recipient_keys.npk(),
                    SharedSecretKey::new(&[56; 32], &recipient_keys.ivk()),
                ),
            ],
            &private_account_auth,
            &program,
        );

        assert!(matches!(result, Err(NssaError::CircuitProvingError(_))));
    }

    #[test]
    fn test_private_accounts_can_only_be_initialized_once() {
        let sender_keys = test_private_account_keys_1();
        let sender_private_account = Account {
            program_owner: Program::authenticated_transfer_program().id(),
            balance: 100,
            nonce: 0xdeadbeef,
            data: vec![],
        };
        let recipient_keys = test_private_account_keys_2();

        let mut state = V02State::new_with_genesis_accounts(&[], &[])
            .with_private_account(&sender_keys, &sender_private_account);

        let balance_to_move = 37;

        let tx = private_balance_transfer_for_tests(
            &sender_keys,
            &sender_private_account,
            &recipient_keys,
            balance_to_move,
            [0xcafecafe, 0xfecafeca],
            &state,
        );

        state
            .transition_from_privacy_preserving_transaction(&tx)
            .unwrap();

        let sender_private_account = Account {
            program_owner: Program::authenticated_transfer_program().id(),
            balance: 100 - balance_to_move,
            nonce: 0xcafecafe,
            data: vec![],
        };

        let tx = private_balance_transfer_for_tests(
            &sender_keys,
            &sender_private_account,
            &recipient_keys,
            balance_to_move,
            [0x1234, 0x5678],
            &state,
        );

        let result = state.transition_from_privacy_preserving_transaction(&tx);

        assert!(matches!(result, Err(NssaError::InvalidInput(_))));
        let NssaError::InvalidInput(error_message) = result.err().unwrap() else {
            panic!("Incorrect message error");
        };
        let expected_error_message = "Nullifier already seen".to_string();
        assert_eq!(error_message, expected_error_message);
    }

    #[test]
    fn test_circuit_should_fail_if_there_are_repeated_ids() {
        let program = Program::simple_balance_transfer();
        let sender_keys = test_private_account_keys_1();
        let private_account_1 = AccountWithMetadata::new(
            Account {
                program_owner: program.id(),
                balance: 100,
                ..Account::default()
            },
            true,
            &sender_keys.npk(),
        );

        let visibility_mask = [1, 1];
        let private_account_auth = [
            (sender_keys.nsk, (1, vec![])),
            (sender_keys.nsk, (1, vec![])),
        ];
        let shared_secret = SharedSecretKey::new(&[55; 32], &sender_keys.ivk());
        let result = execute_and_prove(
            &[private_account_1.clone(), private_account_1],
            &Program::serialize_instruction(100u128).unwrap(),
            &visibility_mask,
            &[0xdeadbeef1, 0xdeadbeef2],
            &[
                (sender_keys.npk(), shared_secret.clone()),
                (sender_keys.npk(), shared_secret),
            ],
            &private_account_auth,
            &program,
        );

        assert!(matches!(result, Err(NssaError::CircuitProvingError(_))));
    }
}

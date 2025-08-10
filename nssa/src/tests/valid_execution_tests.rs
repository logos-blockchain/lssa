use nssa_core::account::Account;

use crate::{
    Address, PublicTransaction, V01State, error::NssaError, program::Program, public_transaction,
};

#[test]
fn test_program_should_fail_if_modifies_nonces() {
    let initial_data = [([1; 32], 100)];
    let mut state = V01State::new_with_genesis_accounts(&initial_data).with_test_programs();
    let addresses = vec![Address::new([1; 32])];
    let program_id = Program::nonce_changer_program().id();
    let message = public_transaction::Message::new(program_id, addresses, vec![], vec![]);
    let witness_set = public_transaction::WitnessSet::for_message(&message, &[]);
    let tx = PublicTransaction::new(message, witness_set);

    let result = state.transition_from_public_transaction(&tx);

    assert!(matches!(result, Err(NssaError::InvalidProgramBehavior)));
}

#[test]
fn test_program_should_fail_if_output_accounts_exceed_inputs() {
    let initial_data = [([1; 32], 100)];
    let mut state = V01State::new_with_genesis_accounts(&initial_data).with_test_programs();
    let addresses = vec![Address::new([1; 32])];
    let program_id = Program::extra_output_program().id();
    let message = public_transaction::Message::new(program_id, addresses, vec![], vec![]);
    let witness_set = public_transaction::WitnessSet::for_message(&message, &[]);
    let tx = PublicTransaction::new(message, witness_set);

    let result = state.transition_from_public_transaction(&tx);

    assert!(matches!(result, Err(NssaError::InvalidProgramBehavior)));
}

#[test]
fn test_program_should_fail_with_missing_output_accounts() {
    let initial_data = [([1; 32], 100)];
    let mut state = V01State::new_with_genesis_accounts(&initial_data).with_test_programs();
    let addresses = vec![Address::new([1; 32]), Address::new([2; 32])];
    let program_id = Program::missing_output_program().id();
    let message = public_transaction::Message::new(program_id, addresses, vec![], vec![]);
    let witness_set = public_transaction::WitnessSet::for_message(&message, &[]);
    let tx = PublicTransaction::new(message, witness_set);

    let result = state.transition_from_public_transaction(&tx);

    assert!(matches!(result, Err(NssaError::InvalidProgramBehavior)));
}

#[test]
fn test_program_should_fail_if_modifies_program_owner_with_only_non_default_program_owner() {
    let initial_data = [([1; 32], 0)];
    let mut state = V01State::new_with_genesis_accounts(&initial_data).with_test_programs();
    let address = Address::new([1; 32]);
    let account = state.get_account_by_address(&address);
    // Assert the target account only differs from the default account in the program owner field
    assert_ne!(account.program_owner, Account::default().program_owner);
    assert_eq!(account.balance, Account::default().balance);
    assert_eq!(account.nonce, Account::default().nonce);
    assert_eq!(account.data, Account::default().data);
    let program_id = Program::program_owner_changer().id();
    let message = public_transaction::Message::new(program_id, vec![address], vec![], vec![]);
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
    let message = public_transaction::Message::new(program_id, vec![address], vec![], vec![]);
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
    let message = public_transaction::Message::new(program_id, vec![address], vec![], vec![]);
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
    let message = public_transaction::Message::new(program_id, vec![address], vec![], vec![]);
    let witness_set = public_transaction::WitnessSet::for_message(&message, &[]);
    let tx = PublicTransaction::new(message, witness_set);

    let result = state.transition_from_public_transaction(&tx);

    assert!(matches!(result, Err(NssaError::InvalidProgramBehavior)));
}

#[test]
fn test_program_should_fail_if_transfers_balance_from_non_owned_account() {
    let initial_data = [([1; 32], 100)];
    let mut state = V01State::new_with_genesis_accounts(&initial_data).with_test_programs();
    let sender_address = Address::new([1; 32]);
    let receiver_address = Address::new([2; 32]);
    let balance_to_move: u128 = 1;
    let program_id = Program::simple_balance_transfer().id();
    let instruction_data = Program::serialize_instruction_data(balance_to_move).unwrap();
    assert_ne!(
        state.get_account_by_address(&sender_address).program_owner,
        program_id
    );
    let message = public_transaction::Message::new(
        program_id,
        vec![sender_address, receiver_address],
        vec![],
        instruction_data,
    );
    let witness_set = public_transaction::WitnessSet::for_message(&message, &[]);
    let tx = PublicTransaction::new(message, witness_set);

    let result = state.transition_from_public_transaction(&tx);
    println!("{:?}", result);

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
    let message = public_transaction::Message::new(program_id, vec![address], vec![], vec![]);
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

    let message = public_transaction::Message::new(program_id, vec![address], vec![], vec![]);
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
    let balance_to_burn = 1;
    assert!(state.get_account_by_address(&address).balance > balance_to_burn);

    let instruction_data = Program::serialize_instruction_data(balance_to_burn).unwrap();
    let message =
        public_transaction::Message::new(program_id, vec![address], vec![], instruction_data);
    let witness_set = public_transaction::WitnessSet::for_message(&message, &[]);
    let tx = PublicTransaction::new(message, witness_set);
    let result = state.transition_from_public_transaction(&tx);

    assert!(matches!(result, Err(NssaError::InvalidProgramBehavior)));
}

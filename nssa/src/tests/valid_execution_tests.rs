use nssa_core::account::Account;

use crate::{
    Address, PublicTransaction, V01State, error::NssaError, program::Program, public_transaction,
};

#[test]
fn test_program_should_fail_if_it_modifies_nonces() {
    let initial_data = [([1; 32], 100)];
    let mut state = V01State::new_with_genesis_accounts(&initial_data).with_test_programs();
    let addresses = vec![Address::new([1; 32])];
    let nonces = vec![];
    let program_id = Program::nonce_changer_program().id();
    let message = public_transaction::Message::new(program_id, addresses, nonces, 0);
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
    let nonces = vec![];
    let program_id = Program::extra_output_program().id();
    let message = public_transaction::Message::new(program_id, addresses, nonces, 0);
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
    let nonces = vec![];
    let program_id = Program::missing_output_program().id();
    let message = public_transaction::Message::new(program_id, addresses, nonces, 0);
    let witness_set = public_transaction::WitnessSet::for_message(&message, &[]);
    let tx = PublicTransaction::new(message, witness_set);

    let result = state.transition_from_public_transaction(&tx);

    assert!(matches!(result, Err(NssaError::InvalidProgramBehavior)));
}

#[test]
fn test_program_should_fail_if_it_modifies_program_owner_with_only_non_default_program_owner() {
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
    let message = public_transaction::Message::new(program_id, vec![address], vec![], 0);
    let witness_set = public_transaction::WitnessSet::for_message(&message, &[]);
    let tx = PublicTransaction::new(message, witness_set);

    let result = state.transition_from_public_transaction(&tx);

    assert!(matches!(result, Err(NssaError::InvalidProgramBehavior)));
}

#[test]
fn test_program_should_fail_if_it_modifies_program_owner_with_only_non_default_balance() {
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
    let message = public_transaction::Message::new(program_id, vec![address], vec![], 0);
    let witness_set = public_transaction::WitnessSet::for_message(&message, &[]);
    let tx = PublicTransaction::new(message, witness_set);

    let result = state.transition_from_public_transaction(&tx);

    assert!(matches!(result, Err(NssaError::InvalidProgramBehavior)));
}

#[test]
fn test_program_should_fail_if_it_modifies_program_owner_with_only_non_default_nonce() {
    let initial_data = [];
    let mut state = V01State::new_with_genesis_accounts(&initial_data)
        .with_test_programs()
        .with_non_default_accounts_but_default_program_owners();
    let address = Address::new([254; 32]);
    let account = state.get_account_by_address(&address);
    // Assert the target account only differs from the default account in balance field
    assert_eq!(account.program_owner, Account::default().program_owner);
    assert_eq!(account.balance, Account::default().balance);
    assert_ne!(account.nonce, Account::default().nonce);
    assert_eq!(account.data, Account::default().data);
    let program_id = Program::program_owner_changer().id();
    let message = public_transaction::Message::new(program_id, vec![address], vec![], 0);
    let witness_set = public_transaction::WitnessSet::for_message(&message, &[]);
    let tx = PublicTransaction::new(message, witness_set);

    let result = state.transition_from_public_transaction(&tx);

    assert!(matches!(result, Err(NssaError::InvalidProgramBehavior)));
}

#[test]
fn test_program_should_fail_if_it_modifies_program_owner_with_only_non_default_data() {
    let initial_data = [];
    let mut state = V01State::new_with_genesis_accounts(&initial_data)
        .with_test_programs()
        .with_non_default_accounts_but_default_program_owners();
    let address = Address::new([253; 32]);
    let account = state.get_account_by_address(&address);
    // Assert the target account only differs from the default account in balance field
    assert_eq!(account.program_owner, Account::default().program_owner);
    assert_eq!(account.balance, Account::default().balance);
    assert_eq!(account.nonce, Account::default().nonce);
    assert_ne!(account.data, Account::default().data);
    let program_id = Program::program_owner_changer().id();
    let message = public_transaction::Message::new(program_id, vec![address], vec![], 0);
    let witness_set = public_transaction::WitnessSet::for_message(&message, &[]);
    let tx = PublicTransaction::new(message, witness_set);

    let result = state.transition_from_public_transaction(&tx);

    assert!(matches!(result, Err(NssaError::InvalidProgramBehavior)));
}

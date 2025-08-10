use crate::{
    Address, PublicTransaction, V01State, error::NssaError, program::Program, public_transaction,
    signature::PrivateKey,
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
    let instruction_data = Program::serialize_instruction_data(&balance).unwrap();
    let message = public_transaction::Message::new(program_id, addresses, nonces, instruction_data);
    let witness_set = public_transaction::WitnessSet::for_message(&message, &[&from_key]);
    PublicTransaction::new(message, witness_set)
}

#[test]
fn transition_from_authenticated_transfer_program_invocation_default_account_destination() {
    let initial_data = [([1; 32], 100)];
    let mut state = V01State::new_with_genesis_accounts(&initial_data);
    let from = Address::new(initial_data[0].0);
    let from_key = PrivateKey(1);
    let to = Address::new([2; 32]);
    assert_eq!(state.get_account_by_address(&to), Account::default());
    let balance_to_move = 5;

    let tx = transfer_transaction(from.clone(), from_key, 0, to.clone(), balance_to_move);
    state.transition_from_public_transaction(&tx).unwrap();

    assert_eq!(state.get_account_by_address(&from).balance, 95);
    assert_eq!(state.get_account_by_address(&to).balance, 5);
    assert_eq!(state.get_account_by_address(&from).nonce, 1);
    assert_eq!(state.get_account_by_address(&to).nonce, 0);
}

#[test]
fn transition_from_authenticated_transfer_program_invocation_insuficient_balance() {
    let initial_data = [([1; 32], 100)];
    let mut state = V01State::new_with_genesis_accounts(&initial_data);
    let from = Address::new(initial_data[0].0);
    let from_key = PrivateKey(1);
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
    let initial_data = [([1; 32], 100), ([99; 32], 200)];
    let mut state = V01State::new_with_genesis_accounts(&initial_data);
    let from = Address::new(initial_data[1].0);
    let from_key = PrivateKey(99);
    let to = Address::new(initial_data[0].0);
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
    let initial_data = [([1; 32], 100)];
    let mut state = V01State::new_with_genesis_accounts(&initial_data);
    let address_1 = Address::new(initial_data[0].0);
    let key_1 = PrivateKey(1);
    let address_2 = Address::new([2; 32]);
    let key_2 = PrivateKey(2);
    let address_3 = Address::new([3; 32]);
    let balance_to_move = 5;

    let tx = transfer_transaction(
        address_1.clone(),
        key_1,
        0,
        address_2.clone(),
        balance_to_move,
    );
    state.transition_from_public_transaction(&tx).unwrap();
    let balance_to_move = 3;
    let tx = transfer_transaction(
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

use nssa;

use crate::block::{Block, HashableBlockData};

//Dummy producers

///Produce dummy block with
///
/// `id` - block id, provide zero for genesis
///
/// `prev_hash` - hash of previous block, provide None for genesis
///
/// `transactions` - vector of `Transaction` objects
pub fn produce_dummy_block(
    id: u64,
    prev_hash: Option<[u8; 32]>,
    transactions: Vec<nssa::PublicTransaction>,
) -> Block {
    let block_data = HashableBlockData {
        block_id: id,
        prev_block_id: id.saturating_sub(1),
        prev_block_hash: prev_hash.unwrap_or_default(),
        transactions,
    };

    block_data.into()
}

pub fn produce_dummy_empty_transaction() -> nssa::PublicTransaction {
    let program_id = nssa::program::Program::authenticated_transfer_program().id();
    let addresses = vec![];
    let nonces = vec![];
    let instruction_data: u128 = 0;
    let message =
        nssa::public_transaction::Message::try_new(program_id, addresses, nonces, instruction_data)
            .unwrap();
    let private_key = nssa::PrivateKey::try_new([1; 32]).unwrap();
    let witness_set = nssa::public_transaction::WitnessSet::for_message(&message, &[&private_key]);
    nssa::PublicTransaction::new(message, witness_set)
}

pub fn create_transaction_native_token_transfer(
    from: [u8; 32],
    nonce: u128,
    to: [u8; 32],
    balance_to_move: u128,
    signing_key: nssa::PrivateKey,
) -> nssa::PublicTransaction {
    let addresses = vec![nssa::Address::new(from), nssa::Address::new(to)];
    let nonces = vec![nonce];
    let program_id = nssa::program::Program::authenticated_transfer_program().id();
    let message =
        nssa::public_transaction::Message::try_new(program_id, addresses, nonces, balance_to_move)
            .unwrap();
    let witness_set = nssa::public_transaction::WitnessSet::for_message(&message, &[&signing_key]);
    nssa::PublicTransaction::new(message, witness_set)
}

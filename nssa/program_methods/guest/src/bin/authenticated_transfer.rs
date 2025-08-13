use nssa_core::program::read_nssa_inputs;
use risc0_zkvm::guest::env;

type Instruction = u128;

/// A transfer of balance program.
/// To be used both in public and private contexts.
fn main() {
    // Read input accounts.
    // It is expected to receive only two accounts: [sender_account, receiver_account]
    let (input_accounts, balance_to_move) = read_nssa_inputs::<Instruction>();

    // Continue only if input_accounts is an array of two elements
    let [sender, receiver] = match input_accounts.try_into() {
        Ok(array) => array,
        Err(_) => return,
    };

    // Continue only if the sender has authorized this operation
    if !sender.is_authorized {
        return;
    }

    // Continue only if the sender has enough balance
    if sender.account.balance < balance_to_move {
        return;
    }

    // Create accounts post states, with updated balances
    let mut sender_post = sender.account.clone();
    let mut receiver_post = receiver.account.clone();
    sender_post.balance -= balance_to_move;
    receiver_post.balance += balance_to_move;

    env::commit(&vec![sender_post, receiver_post]);
}

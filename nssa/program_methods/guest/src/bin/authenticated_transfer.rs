use nssa_core::{
    account::{Account, AccountWithMetadata},
    program::{ProgramInput, read_nssa_inputs, write_nssa_outputs},
};

/// Initializes a default account under the ownership of this program.
/// This is achieved by a noop.
fn initialize_account(pre_state: AccountWithMetadata) {
    let account_to_claim = pre_state.account.clone();
    let is_authorized = pre_state.is_authorized;

    // Continue only if the account to claim has default values
    if account_to_claim != Account::default() {
        return;
    }

    // Continue only if the owner authorized this operation
    if !is_authorized {
        return;
    }

    // Noop will result in account being claimed for this program
    write_nssa_outputs(vec![pre_state], vec![account_to_claim]);
}

/// Transfers `balance_to_move` native balance from `sender` to `recipient`.
fn transfer(sender: AccountWithMetadata, recipient: AccountWithMetadata, balance_to_move: u128) {
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
    let mut recipient_post = recipient.account.clone();
    sender_post.balance -= balance_to_move;
    recipient_post.balance += balance_to_move;

    write_nssa_outputs(vec![sender, recipient], vec![sender_post, recipient_post]);
}

/// A transfer of balance program.
/// To be used both in public and private contexts.
fn main() {
    // Read input accounts.
    let ProgramInput {
        pre_states,
        instruction: balance_to_move,
    } = read_nssa_inputs();

    match (pre_states.as_slice(), balance_to_move) {
        ([account_to_claim], 0) => initialize_account(account_to_claim.clone()),
        ([sender, recipient], balance_to_move) => {
            transfer(sender.clone(), recipient.clone(), balance_to_move)
        }
        _ => panic!("invalid params"),
    }
}

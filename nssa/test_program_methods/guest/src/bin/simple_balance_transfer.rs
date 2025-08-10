use nssa_core::program::read_nssa_inputs;
use risc0_zkvm::guest::env;

type Instruction = u128;

fn main() {
    let (input_accounts, balance) = read_nssa_inputs::<Instruction>();

    let [sender_pre, receiver_pre] = match input_accounts.try_into() {
        Ok(array) => array,
        Err(_) => return,
    };

    let mut sender_post = sender_pre.account.clone();
    let mut receiver_post = receiver_pre.account.clone();
    sender_post.balance -= balance;
    receiver_post.balance += balance;

    env::commit(&vec![sender_post, receiver_post]);
}

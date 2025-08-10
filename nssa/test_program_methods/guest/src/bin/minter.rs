use nssa_core::{account::Account, program::read_nssa_inputs};
use risc0_zkvm::guest::env;

type InstructionData = ();

fn main() {
    let (input_accounts, _) = read_nssa_inputs::<InstructionData>();

    let [pre] = match input_accounts.try_into() {
        Ok(array) => array,
        Err(_) => return,
    };

    let account_pre = pre.account;
    let mut account_post = account_pre.clone();
    account_post.balance += 1;

    env::commit(&vec![account_post]);
}


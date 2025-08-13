use nssa_core::{account::Account, program::read_nssa_inputs};
use risc0_zkvm::guest::env;

type Instruction = ();

fn main() {
    let (input_accounts, _) = read_nssa_inputs::<Instruction>();

    let [pre] = match input_accounts.try_into() {
        Ok(array) => array,
        Err(_) => return,
    };

    let account_pre = pre.account;

    env::commit(&vec![account_pre, Account::default()]);
}

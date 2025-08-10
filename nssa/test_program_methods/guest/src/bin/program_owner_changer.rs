use nssa_core::account::AccountWithMetadata;
use risc0_zkvm::guest::env;

fn main() {
    let input_accounts: Vec<AccountWithMetadata> = env::read();
    let _instruction_data: u128 = env::read();

    let [pre] = match input_accounts.try_into() {
        Ok(array) => array,
        Err(_) => return,
    };

    let account_pre = pre.account;
    let mut account_post = account_pre.clone();
    account_post.program_owner = [0, 1, 2, 3, 4, 5, 6, 7];

    env::commit(&vec![account_post]);
}

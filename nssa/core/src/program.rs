use crate::account::{Account, AccountWithMetadata};
use risc0_zkvm::serde::Deserializer;
use risc0_zkvm::{DeserializeOwned, guest::env};
use serde::{Deserialize, Serialize};

pub type ProgramId = [u32; 8];
pub type InstructionData = Vec<u32>;
pub const DEFAULT_PROGRAM_ID: ProgramId = [0; 8];

pub struct ProgramInput<T> {
    pub pre_states: Vec<AccountWithMetadata>,
    pub instruction: T,
}

#[derive(Serialize, Deserialize, Clone)]
#[cfg_attr(any(feature = "host", test), derive(Debug, PartialEq, Eq))]
pub struct ProgramOutput {
    pub pre_states: Vec<AccountWithMetadata>,
    pub post_states: Vec<Account>,
}

pub fn read_nssa_inputs<T: DeserializeOwned>() -> ProgramInput<T> {
    let pre_states: Vec<AccountWithMetadata> = env::read();
    let words: InstructionData = env::read();
    let instruction = T::deserialize(&mut Deserializer::new(words.as_ref())).unwrap();
    ProgramInput {
        pre_states,
        instruction,
    }
}

pub fn write_nssa_outputs(pre_states: Vec<AccountWithMetadata>, post_states: Vec<Account>) {
    let output = ProgramOutput {
        pre_states,
        post_states,
    };
    env::commit(&output);
}

/// Validates well-behaved program execution
///
/// # Parameters
/// - `pre_states`: The list of input accounts, each annotated with authorization metadata.
/// - `post_states`: The list of resulting accounts after executing the program logic.
/// - `executing_program_id`: The identifier of the program that was executed.
pub fn validate_execution(
    pre_states: &[AccountWithMetadata],
    post_states: &[Account],
    executing_program_id: ProgramId,
) -> bool {
    // 1. Lengths must match
    if pre_states.len() != post_states.len() {
        return false;
    }

    for (pre, post) in pre_states.iter().zip(post_states) {
        // 2. Nonce must remain unchanged
        if pre.account.nonce != post.nonce {
            return false;
        }

        // 3. Ownership change only allowed from default accounts
        if pre.account.program_owner != post.program_owner && pre.account != Account::default() {
            return false;
        }

        // 4. Decreasing balance only allowed if owned by executing program
        if post.balance < pre.account.balance && pre.account.program_owner != executing_program_id {
            return false;
        }

        // 5. Data changes only allowed if owned by executing program
        if pre.account.data != post.data
            && (executing_program_id != pre.account.program_owner
                || executing_program_id != post.program_owner)
        {
            return false;
        }
    }

    // 6. Total balance is preserved
    let total_balance_pre_states: u128 = pre_states.iter().map(|pre| pre.account.balance).sum();
    let total_balance_post_states: u128 = post_states.iter().map(|post| post.balance).sum();
    if total_balance_pre_states != total_balance_post_states {
        return false;
    }

    true
}

use risc0_zkvm::{DeserializeOwned, guest::env, serde::Deserializer};
use serde::{Deserialize, Serialize};

use crate::account::{Account, AccountWithMetadata};

pub type ProgramId = [u32; 8];
pub type InstructionData = Vec<u32>;
pub const DEFAULT_PROGRAM_ID: ProgramId = [0; 8];

pub struct ProgramInput<T> {
    pub pre_states: Vec<AccountWithMetadata>,
    pub instruction: T,
}

#[derive(Serialize, Deserialize, Clone)]
#[cfg_attr(any(feature = "host", test), derive(Debug, PartialEq, Eq))]
pub struct ChainedCall {
    pub program_id: ProgramId,
    pub instruction_data: InstructionData,
    pub pre_states: Vec<AccountWithMetadata>,
}

/// Represents the final state of an `Account` after a program execution.
/// A post state may optionally request that the executing program
/// becomes the owner of the account (a “claim”). This is used to signal
/// that the program intends to take ownership of the account.
#[derive(Serialize, Deserialize, Clone)]
#[cfg_attr(any(feature = "host", test), derive(Debug, PartialEq, Eq))]
pub struct AccountPostState {
    account: Account,
    claim: bool,
}

impl AccountPostState {
    /// Creates a post state without a claim request.
    /// The executing program is not requesting ownership of the account.
    pub fn new(account: Account) -> Self {
        Self {
            account,
            claim: false,
        }
    }

    /// Creates a post state that requests ownership of the account.
    /// This indicates that the executing program intends to claim the
    /// account as its own and is allowed to mutate it.
    pub fn new_claimed(account: Account) -> Self {
        Self {
            account,
            claim: true,
        }
    }

    /// Returns `true` if this post state requests that the account
    /// be claimed (owned) by the executing program.
    pub fn requires_claim(&self) -> bool {
        self.claim
    }

    /// Returns the underlying account
    pub fn account(&self) -> &Account {
        &self.account
    }

    /// Returns the underlying account
    pub fn account_mut(&mut self) -> &mut Account {
        &mut self.account
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[cfg_attr(any(feature = "host", test), derive(Debug, PartialEq, Eq))]
pub struct ProgramOutput {
    pub pre_states: Vec<AccountWithMetadata>,
    pub post_states: Vec<AccountPostState>,
    pub chained_calls: Vec<ChainedCall>,
}

pub fn read_nssa_inputs<T: DeserializeOwned>() -> ProgramInput<T> {
    let pre_states: Vec<AccountWithMetadata> = env::read();
    let instruction_words: InstructionData = env::read();
    let instruction = T::deserialize(&mut Deserializer::new(instruction_words.as_ref())).unwrap();
    ProgramInput {
        pre_states,
        instruction,
    }
}

pub fn write_nssa_outputs(
    pre_states: Vec<AccountWithMetadata>,
    post_states: Vec<AccountPostState>,
) {
    let output = ProgramOutput {
        pre_states,
        post_states,
        chained_calls: Vec::new(),
    };
    env::commit(&output);
}

pub fn write_nssa_outputs_with_chained_call(
    pre_states: Vec<AccountWithMetadata>,
    post_states: Vec<AccountPostState>,
    chained_calls: Vec<ChainedCall>,
) {
    let output = ProgramOutput {
        pre_states,
        post_states,
        chained_calls,
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
    post_states: &[AccountPostState],
    executing_program_id: ProgramId,
) -> bool {
    // 1. Lengths must match
    if pre_states.len() != post_states.len() {
        return false;
    }

    for (pre, post) in pre_states.iter().zip(post_states) {
        // 2. Nonce must remain unchanged
        if pre.account.nonce != post.account.nonce {
            return false;
        }

        // 3. Program ownership changes are not allowed
        if pre.account.program_owner != post.account.program_owner {
            return false;
        }

        let account_program_owner = pre.account.program_owner;

        // 4. Decreasing balance only allowed if owned by executing program
        if post.account.balance < pre.account.balance
            && account_program_owner != executing_program_id
        {
            return false;
        }

        // 5. Data changes only allowed if owned by executing program or if account pre state has
        //    default values
        if pre.account.data != post.account.data
            && pre.account != Account::default()
            && account_program_owner != executing_program_id
        {
            return false;
        }

        // 6. If a post state has default program owner, the pre state must have been a default
        //    account
        if post.account.program_owner == DEFAULT_PROGRAM_ID && pre.account != Account::default() {
            return false;
        }
    }

    // 7. Total balance is preserved
    let total_balance_pre_states: u128 = pre_states.iter().map(|pre| pre.account.balance).sum();
    let total_balance_post_states: u128 = post_states.iter().map(|post| post.account.balance).sum();
    if total_balance_pre_states != total_balance_post_states {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_post_state_new_with_claim_constructor() {
        let account = Account {
            program_owner: [1, 2, 3, 4, 5, 6, 7, 8],
            balance: 1337,
            data: vec![0xde, 0xad, 0xbe, 0xef],
            nonce: 10,
        };

        let account_post_state = AccountPostState::new_claimed(account.clone());

        assert_eq!(account, account_post_state.account);
        assert!(account_post_state.requires_claim());
    }

    #[test]
    fn test_post_state_new_without_claim_constructor() {
        let account = Account {
            program_owner: [1, 2, 3, 4, 5, 6, 7, 8],
            balance: 1337,
            data: vec![0xde, 0xad, 0xbe, 0xef],
            nonce: 10,
        };

        let account_post_state = AccountPostState::new(account.clone());

        assert_eq!(account, account_post_state.account);
        assert!(!account_post_state.requires_claim());
    }

    #[test]
    fn test_post_state_account_getter() {
        let mut account = Account {
            program_owner: [1, 2, 3, 4, 5, 6, 7, 8],
            balance: 1337,
            data: vec![0xde, 0xad, 0xbe, 0xef],
            nonce: 10,
        };

        let mut account_post_state = AccountPostState::new(account.clone());

        assert_eq!(account_post_state.account(), &account);
        assert_eq!(account_post_state.account_mut(), &mut account);
    }
}

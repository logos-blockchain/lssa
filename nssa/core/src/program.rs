use risc0_zkvm::{DeserializeOwned, guest::env, serde::Deserializer};
use serde::{Deserialize, Serialize};

use crate::account::{Account, AccountId, AccountWithMetadata};

pub type ProgramId = [u32; 8];
pub type InstructionData = Vec<u32>;
pub const DEFAULT_PROGRAM_ID: ProgramId = [0; 8];

pub struct ProgramInput<T> {
    pub pre_states: Vec<AccountWithMetadata>,
    pub instruction: T,
}

#[derive(Serialize, Deserialize, Clone)]
#[cfg_attr(any(feature = "host", test), derive(Debug, PartialEq, Eq))]
pub struct PdaSeed([u8; 32]);

#[cfg(feature = "host")]
impl From<(&ProgramId, &PdaSeed)> for AccountId {
    fn from(value: (&ProgramId, &PdaSeed)) -> Self {
        use risc0_zkvm::sha::{Impl, Sha256};
        const PROGRAM_DERIVED_ACCOUNT_ID_PREFIX: &[u8; 32] =
            b"/NSSA/v0.2/AccountId/PDA/\x00\x00\x00\x00\x00\x00\x00";

        let mut bytes = [0; 96];
        bytes[0..32].copy_from_slice(PROGRAM_DERIVED_ACCOUNT_ID_PREFIX);
        let program_id_bytes: &[u8] =
            bytemuck::try_cast_slice(value.0).expect("ProgramId should be castable to &[u8]");
        bytes[32..64].copy_from_slice(program_id_bytes);
        bytes[64..].copy_from_slice(&value.1.0);
        AccountId::new(
            Impl::hash_bytes(&bytes)
                .as_bytes()
                .try_into()
                .expect("Hash output must be exactly 32 bytes long"),
        )
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[cfg_attr(any(feature = "host", test), derive(Debug, PartialEq, Eq))]
pub struct ChainedCall {
    pub program_id: ProgramId,
    pub instruction_data: InstructionData,
    pub pre_states: Vec<AccountWithMetadata>,
    pub pda_seeds: Vec<PdaSeed>,
}

#[derive(Serialize, Deserialize, Clone)]
#[cfg_attr(any(feature = "host", test), derive(Debug, PartialEq, Eq))]
pub struct ProgramOutput {
    pub pre_states: Vec<AccountWithMetadata>,
    pub post_states: Vec<Account>,
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

pub fn write_nssa_outputs(pre_states: Vec<AccountWithMetadata>, post_states: Vec<Account>) {
    let output = ProgramOutput {
        pre_states,
        post_states,
        chained_calls: Vec::new(),
    };
    env::commit(&output);
}

pub fn write_nssa_outputs_with_chained_call(
    pre_states: Vec<AccountWithMetadata>,
    post_states: Vec<Account>,
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

        // 3. Program ownership changes are not allowed
        if pre.account.program_owner != post.program_owner {
            return false;
        }

        let account_program_owner = pre.account.program_owner;

        // 4. Decreasing balance only allowed if owned by executing program
        if post.balance < pre.account.balance && account_program_owner != executing_program_id {
            return false;
        }

        // 5. Data changes only allowed if owned by executing program or if account pre state has
        //    default values
        if pre.account.data != post.data
            && pre.account != Account::default()
            && account_program_owner != executing_program_id
        {
            return false;
        }

        // 6. If a post state has default program owner, the pre state must have been a default
        //    account
        if post.program_owner == DEFAULT_PROGRAM_ID && pre.account != Account::default() {
            return false;
        }
    }

    // 7. Total balance is preserved
    let total_balance_pre_states: u128 = pre_states.iter().map(|pre| pre.account.balance).sum();
    let total_balance_post_states: u128 = post_states.iter().map(|post| post.balance).sum();
    if total_balance_pre_states != total_balance_post_states {
        return false;
    }

    true
}

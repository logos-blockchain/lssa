use nssa_core::{
    account::{Account, AccountWithMetadata},
    program::DEFAULT_PROGRAM_ID,
};
use program_methods::{AUTHENTICATED_TRANSFER_ELF, AUTHENTICATED_TRANSFER_ID};
use risc0_zkvm::{ExecutorEnv, ExecutorEnvBuilder, default_executor};

mod address;
pub mod error;
pub mod public_transaction;
mod signature;
mod state;

pub use address::Address;
pub use nssa_core::program::Program;
pub use public_transaction::PublicTransaction;
pub use signature::PrivateKey;
pub use signature::PublicKey;
pub use signature::Signature;
pub use state::V01State;

use crate::error::NssaError;

pub const AUTHENTICATED_TRANSFER_PROGRAM: Program = Program {
    id: AUTHENTICATED_TRANSFER_ID,
    elf: AUTHENTICATED_TRANSFER_ELF,
};

/// Writes inputs to `env_builder` in the order expected by the programs
fn write_inputs(
    pre_states: &[AccountWithMetadata],
    instruction_data: u128,
    env_builder: &mut ExecutorEnvBuilder,
) -> Result<(), NssaError> {
    let pre_states = pre_states.to_vec();
    env_builder
        .write(&pre_states)
        .map_err(|e| NssaError::ProgramExecutionFailed(e.to_string()))?;
    env_builder
        .write(&instruction_data)
        .map_err(|e| NssaError::ProgramExecutionFailed(e.to_string()))?;
    Ok(())
}

fn execute_public(
    pre_states: &[AccountWithMetadata],
    instruction_data: u128,
    program: &Program,
) -> Result<Vec<Account>, NssaError> {
    // Write inputs to the program
    let mut env_builder = ExecutorEnv::builder();
    write_inputs(pre_states, instruction_data, &mut env_builder)?;
    let env = env_builder.build().unwrap();

    // Execute the program (without proving)
    let executor = default_executor();
    let session_info = executor
        .execute(env, program.elf)
        .map_err(|e| NssaError::ProgramExecutionFailed(e.to_string()))?;

    // Get outputs
    let mut post_states: Vec<Account> = session_info
        .journal
        .decode()
        .map_err(|e| NssaError::ProgramExecutionFailed(e.to_string()))?;

    // Claim any output account with default program owner field
    for account in post_states.iter_mut() {
        if account.program_owner == DEFAULT_PROGRAM_ID {
            account.program_owner = program.id;
        }
    }

    Ok(post_states)
}

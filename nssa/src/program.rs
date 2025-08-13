use nssa_core::{
    account::{Account, AccountWithMetadata},
    program::{DEFAULT_PROGRAM_ID, InstructionData, ProgramId},
};
use program_methods::{AUTHENTICATED_TRANSFER_ELF, AUTHENTICATED_TRANSFER_ID};
use risc0_zkvm::{ExecutorEnv, ExecutorEnvBuilder, default_executor, serde::to_vec};
use serde::Serialize;

use crate::error::NssaError;

pub struct Program {
    pub(crate) id: ProgramId,
    pub(crate) elf: &'static [u8],
}

impl Program {
    pub fn id(&self) -> ProgramId {
        self.id
    }

    pub fn serialize_instruction<T: Serialize>(
        instruction: T,
    ) -> Result<InstructionData, NssaError> {
        to_vec(&instruction).map_err(|e| NssaError::InstructionSerializationError(e.to_string()))
    }

    pub(crate) fn execute(
        &self,
        pre_states: &[AccountWithMetadata],
        instruction_data: &InstructionData,
    ) -> Result<Vec<Account>, NssaError> {
        // Write inputs to the program
        let mut env_builder = ExecutorEnv::builder();
        Self::write_inputs(pre_states, instruction_data, &mut env_builder)?;
        let env = env_builder.build().unwrap();

        // Execute the program (without proving)
        let executor = default_executor();
        let session_info = executor
            .execute(env, self.elf)
            .map_err(|e| NssaError::ProgramExecutionFailed(e.to_string()))?;

        // Get outputs
        let mut post_states: Vec<Account> = session_info
            .journal
            .decode()
            .map_err(|e| NssaError::ProgramExecutionFailed(e.to_string()))?;

        // Claim any output account with default program owner field
        for account in post_states.iter_mut() {
            if account.program_owner == DEFAULT_PROGRAM_ID {
                account.program_owner = self.id;
            }
        }

        Ok(post_states)
    }

    /// Writes inputs to `env_builder` in the order expected by the programs
    fn write_inputs(
        pre_states: &[AccountWithMetadata],
        instruction_data: &[u32],
        env_builder: &mut ExecutorEnvBuilder,
    ) -> Result<(), NssaError> {
        let pre_states = pre_states.to_vec();
        env_builder
            .write(&(pre_states, instruction_data))
            .map_err(|e| NssaError::ProgramExecutionFailed(e.to_string()))?;
        Ok(())
    }

    pub fn authenticated_transfer_program() -> Self {
        Self {
            id: AUTHENTICATED_TRANSFER_ID,
            elf: AUTHENTICATED_TRANSFER_ELF,
        }
    }
}

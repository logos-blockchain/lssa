use nssa_core::{
    account::{Account, AccountWithMetadata},
    program::{DEFAULT_PROGRAM_ID, ProgramId},
};
use program_methods::{AUTHENTICATED_TRANSFER_ELF, AUTHENTICATED_TRANSFER_ID};
use risc0_zkvm::{ExecutorEnv, ExecutorEnvBuilder, default_executor};

use crate::error::NssaError;

pub struct Program {
    pub(crate) id: ProgramId,
    pub(crate) elf: &'static [u8],
}

impl Program {
    pub fn id(&self) -> ProgramId {
        self.id
    }

    pub(crate) fn execute(
        &self,
        pre_states: &[AccountWithMetadata],
        instruction_data: u128,
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

    pub fn authenticated_transfer_program() -> Self {
        Self {
            id: AUTHENTICATED_TRANSFER_ID,
            elf: AUTHENTICATED_TRANSFER_ELF,
        }
    }
}

// Test utils
#[cfg(test)]
impl Program {
    /// A program that changes the nonce of an account
    pub fn nonce_changer_program() -> Self {
        use test_program_methods::{NONCE_CHANGER_ELF, NONCE_CHANGER_ID};

        Program {
            id: NONCE_CHANGER_ID,
            elf: NONCE_CHANGER_ELF,
        }
    }

    /// A program that produces more output accounts than the inputs it received
    pub fn extra_output_program() -> Self {
        use test_program_methods::{EXTRA_OUTPUT_ELF, EXTRA_OUTPUT_ID};

        Program {
            id: EXTRA_OUTPUT_ID,
            elf: EXTRA_OUTPUT_ELF,
        }
    }

    /// A program that produces less output accounts than the inputs it received
    pub fn missing_output_program() -> Self {
        use test_program_methods::{MISSING_OUTPUT_ELF, MISSING_OUTPUT_ID};

        Program {
            id: MISSING_OUTPUT_ID,
            elf: MISSING_OUTPUT_ELF,
        }
    }

    /// A program that changes the program owner of an account to [0, 1, 2, 3, 4, 5, 6, 7]
    pub fn program_owner_changer() -> Self {
        use test_program_methods::{PROGRAM_OWNER_CHANGER_ELF, PROGRAM_OWNER_CHANGER_ID};

        Program {
            id: PROGRAM_OWNER_CHANGER_ID,
            elf: PROGRAM_OWNER_CHANGER_ELF,
        }
    }
}

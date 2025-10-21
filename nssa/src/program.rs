use crate::program_methods::{AUTHENTICATED_TRANSFER_ELF, PINATA_ELF, TOKEN_ELF};
use nssa_core::{
    account::{Account, AccountWithMetadata},
    program::{InstructionData, ProgramId, ProgramOutput},
};

use risc0_zkvm::{ExecutorEnv, ExecutorEnvBuilder, default_executor, serde::to_vec};
use serde::Serialize;

use crate::error::NssaError;

/// Maximum number of cycles for a public execution.
/// TODO: Make this variable when fees are implemented
const MAX_NUM_CYCLES_PUBLIC_EXECUTION: u64 = 1024 * 1024 * 32; // 32M cycles

#[derive(Debug, PartialEq, Eq)]
pub struct Program {
    id: ProgramId,
    elf: Vec<u8>,
}

impl Program {
    pub fn new(bytecode: Vec<u8>) -> Result<Self, NssaError> {
        let binary = risc0_binfmt::ProgramBinary::decode(&bytecode)
            .map_err(|_| NssaError::InvalidProgramBytecode)?;
        let id = binary
            .compute_image_id()
            .map_err(|_| NssaError::InvalidProgramBytecode)?
            .into();
        Ok(Self { elf: bytecode, id })
    }

    pub fn id(&self) -> ProgramId {
        self.id
    }

    pub fn elf(&self) -> &[u8] {
        &self.elf
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
        env_builder.session_limit(Some(MAX_NUM_CYCLES_PUBLIC_EXECUTION));
        Self::write_inputs(pre_states, instruction_data, &mut env_builder)?;
        let env = env_builder.build().unwrap();

        // Execute the program (without proving)
        let executor = default_executor();
        let session_info = executor
            .execute(env, self.elf())
            .map_err(|e| NssaError::ProgramExecutionFailed(e.to_string()))?;

        // Get outputs
        let ProgramOutput { post_states, .. } = session_info
            .journal
            .decode()
            .map_err(|e| NssaError::ProgramExecutionFailed(e.to_string()))?;

        Ok(post_states)
    }

    /// Writes inputs to `env_builder` in the order expected by the programs
    pub(crate) fn write_inputs(
        pre_states: &[AccountWithMetadata],
        instruction_data: &[u32],
        env_builder: &mut ExecutorEnvBuilder,
    ) -> Result<(), NssaError> {
        let pre_states = pre_states.to_vec();
        env_builder
            .write(&(pre_states, instruction_data))
            .map_err(|e| NssaError::ProgramWriteInputFailed(e.to_string()))?;
        Ok(())
    }

    pub fn authenticated_transfer_program() -> Self {
        // This unwrap won't panic since the `AUTHENTICATED_TRANSFER_ELF` comes from risc0 build of
        // `program_methods`
        Self::new(AUTHENTICATED_TRANSFER_ELF.to_vec()).unwrap()
    }

    pub fn token() -> Self {
        // This unwrap won't panic since the `TOKEN_ELF` comes from risc0 build of
        // `program_methods`
        Self::new(TOKEN_ELF.to_vec()).unwrap()
    }
}

// TODO: Testnet only. Refactor to prevent compilation on mainnet.
impl Program {
    pub fn pinata() -> Self {
        // This unwrap won't panic since the `PINATA_ELF` comes from risc0 build of
        // `program_methods`
        Self::new(PINATA_ELF.to_vec()).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use nssa_core::account::{Account, AccountId, AccountWithMetadata};
    use program_methods::{
        AUTHENTICATED_TRANSFER_ELF, AUTHENTICATED_TRANSFER_ID, PINATA_ELF, PINATA_ID, TOKEN_ELF,
        TOKEN_ID,
    };

    use crate::program::Program;

    impl Program {
        /// A program that changes the nonce of an account
        pub fn nonce_changer_program() -> Self {
            use test_program_methods::{NONCE_CHANGER_ELF, NONCE_CHANGER_ID};

            Program {
                id: NONCE_CHANGER_ID,
                elf: NONCE_CHANGER_ELF.to_vec(),
            }
        }

        /// A program that produces more output accounts than the inputs it received
        pub fn extra_output_program() -> Self {
            use test_program_methods::{EXTRA_OUTPUT_ELF, EXTRA_OUTPUT_ID};

            Program {
                id: EXTRA_OUTPUT_ID,
                elf: EXTRA_OUTPUT_ELF.to_vec(),
            }
        }

        /// A program that produces less output accounts than the inputs it received
        pub fn missing_output_program() -> Self {
            use test_program_methods::{MISSING_OUTPUT_ELF, MISSING_OUTPUT_ID};

            Program {
                id: MISSING_OUTPUT_ID,
                elf: MISSING_OUTPUT_ELF.to_vec(),
            }
        }

        /// A program that changes the program owner of an account to [0, 1, 2, 3, 4, 5, 6, 7]
        pub fn program_owner_changer() -> Self {
            use test_program_methods::{PROGRAM_OWNER_CHANGER_ELF, PROGRAM_OWNER_CHANGER_ID};

            Program {
                id: PROGRAM_OWNER_CHANGER_ID,
                elf: PROGRAM_OWNER_CHANGER_ELF.to_vec(),
            }
        }

        /// A program that transfers balance without caring about authorizations
        pub fn simple_balance_transfer() -> Self {
            use test_program_methods::{SIMPLE_BALANCE_TRANSFER_ELF, SIMPLE_BALANCE_TRANSFER_ID};

            Program {
                id: SIMPLE_BALANCE_TRANSFER_ID,
                elf: SIMPLE_BALANCE_TRANSFER_ELF.to_vec(),
            }
        }

        /// A program that modifies the data of an account
        pub fn data_changer() -> Self {
            use test_program_methods::{DATA_CHANGER_ELF, DATA_CHANGER_ID};

            Program {
                id: DATA_CHANGER_ID,
                elf: DATA_CHANGER_ELF.to_vec(),
            }
        }

        /// A program that mints balance
        pub fn minter() -> Self {
            use test_program_methods::{MINTER_ELF, MINTER_ID};

            Program {
                id: MINTER_ID,
                elf: MINTER_ELF.to_vec(),
            }
        }

        /// A program that burns balance
        pub fn burner() -> Self {
            use test_program_methods::{BURNER_ELF, BURNER_ID};

            Program {
                id: BURNER_ID,
                elf: BURNER_ELF.to_vec(),
            }
        }
    }

    #[test]
    fn test_program_execution() {
        let program = Program::simple_balance_transfer();
        let balance_to_move: u128 = 11223344556677;
        let instruction_data = Program::serialize_instruction(balance_to_move).unwrap();
        let sender = AccountWithMetadata::new(
            Account {
                balance: 77665544332211,
                ..Account::default()
            },
            true,
            AccountId::new([0; 32]),
        );
        let recipient =
            AccountWithMetadata::new(Account::default(), false, AccountId::new([1; 32]));

        let expected_sender_post = Account {
            balance: 77665544332211 - balance_to_move,
            ..Account::default()
        };
        let expected_recipient_post = Account {
            balance: balance_to_move,
            ..Account::default()
        };
        let [sender_post, recipient_post] = program
            .execute(&[sender, recipient], &instruction_data)
            .unwrap()
            .try_into()
            .unwrap();

        assert_eq!(sender_post, expected_sender_post);
        assert_eq!(recipient_post, expected_recipient_post);
    }

    #[test]
    fn test_builtin_programs() {
        let auth_transfer_program = Program::authenticated_transfer_program();
        let token_program = Program::token();
        let pinata_program = Program::pinata();

        assert_eq!(auth_transfer_program.id, AUTHENTICATED_TRANSFER_ID);
        assert_eq!(auth_transfer_program.elf, AUTHENTICATED_TRANSFER_ELF);
        assert_eq!(token_program.id, TOKEN_ID);
        assert_eq!(token_program.elf, TOKEN_ELF);
        assert_eq!(pinata_program.id, PINATA_ID);
        assert_eq!(pinata_program.elf, PINATA_ELF);
    }
}

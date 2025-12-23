use common::error::ExecutionFailureKind;
use nssa::{Account, program::Program};
use nssa_core::program::InstructionData;

use crate::{WalletCore, program_facades::ProgramArgs};

pub mod public;

pub struct NativeTokenTransfer<'w>(pub &'w WalletCore);

#[derive(Debug, Clone, Copy)]
pub struct NativeBalanceToMove {
    pub balance_to_move: u128,
}

#[derive(Debug, Clone, Copy)]
pub struct InitArgs {}

impl ProgramArgs for NativeBalanceToMove {
    fn private_transfer_preparation(
        &self,
    ) -> (
        InstructionData,
        Program,
        impl FnOnce(&[&Account]) -> Result<(), ExecutionFailureKind>,
    ) {
        let instruction_data = Program::serialize_instruction(self.balance_to_move).unwrap();
        let program = Program::authenticated_transfer_program();
        let tx_pre_check = move |accounts: &[&Account]| {
            let from = accounts[0];
            if from.balance >= self.balance_to_move {
                Ok(())
            } else {
                Err(ExecutionFailureKind::InsufficientFundsError)
            }
        };

        (instruction_data, program, tx_pre_check)
    }
}

impl ProgramArgs for InitArgs {
    fn private_transfer_preparation(
        &self,
    ) -> (
        InstructionData,
        Program,
        impl FnOnce(&[&Account]) -> Result<(), ExecutionFailureKind>,
    ) {
        (
            Program::serialize_instruction(0u128).unwrap(),
            Program::authenticated_transfer_program(),
            |_| Ok(()),
        )
    }
}

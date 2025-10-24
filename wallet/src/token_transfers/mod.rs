use common::error::ExecutionFailureKind;
use nssa::{Account, program::Program};
use nssa_core::program::InstructionData;

use crate::WalletCore;

pub mod deshielded;
pub mod private;
pub mod public;
pub mod shielded;

impl WalletCore {
    pub fn auth_transfer_preparation(
        balance_to_move: u128,
    ) -> (
        InstructionData,
        Program,
        impl FnOnce(&Account, &Account) -> Result<(), ExecutionFailureKind>,
    ) {
        let instruction_data = Program::serialize_instruction(balance_to_move).unwrap();
        let program = Program::authenticated_transfer_program();
        let tx_pre_check = move |from: &Account, _: &Account| {
            if from.balance >= balance_to_move {
                Ok(())
            } else {
                Err(ExecutionFailureKind::InsufficientFundsError)
            }
        };

        (instruction_data, program, tx_pre_check)
    }
}

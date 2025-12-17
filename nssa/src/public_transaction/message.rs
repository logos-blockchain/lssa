use borsh::{BorshDeserialize, BorshSerialize};
use nssa_core::{
    account::Nonce,
    program::{InstructionData, ProgramId},
};
use serde::Serialize;

use crate::{AccountId, error::NssaError, program::Program};

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Message {
    pub(crate) program_id: ProgramId,
    pub(crate) account_ids: Vec<AccountId>,
    pub(crate) nonces: Vec<Nonce>,
    pub(crate) instruction_data: InstructionData,
}

impl Message {
    pub fn try_new<T: Serialize>(
        program_id: ProgramId,
        account_ids: Vec<AccountId>,
        nonces: Vec<Nonce>,
        instruction: T,
    ) -> Result<Self, NssaError> {
        let instruction_data = Program::serialize_instruction(instruction)?;
        Ok(Self {
            program_id,
            account_ids,
            nonces,
            instruction_data,
        })
    }

    pub fn new_preserialized(
        program_id: ProgramId,
        account_ids: Vec<AccountId>,
        nonces: Vec<Nonce>,
        instruction_data: Vec<u32>,
    ) -> Self {
        Self {
            program_id,
            account_ids,
            nonces,
            instruction_data,
        }
    }
}

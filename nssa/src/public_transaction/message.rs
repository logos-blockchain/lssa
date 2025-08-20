use nssa_core::{
    account::Nonce,
    program::{InstructionData, ProgramId},
};
use serde::Serialize;

use crate::{Address, error::NssaError, program::Program};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub(crate) program_id: ProgramId,
    pub(crate) addresses: Vec<Address>,
    pub(crate) nonces: Vec<Nonce>,
    pub(crate) instruction_data: InstructionData,
}

impl Message {
    pub fn try_new<T: Serialize>(
        program_id: ProgramId,
        addresses: Vec<Address>,
        nonces: Vec<Nonce>,
        instruction: T,
    ) -> Result<Self, NssaError> {
        let instruction_data = Program::serialize_instruction(instruction)?;
        Ok(Self {
            program_id,
            addresses,
            nonces,
            instruction_data,
        })
    }
}

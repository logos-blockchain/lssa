use nssa_core::{
    account::Nonce,
    program::{InstructionData, ProgramId},
};
use serde::{Deserialize, Serialize};

use crate::{Address, program::Program};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Message {
    pub(crate) program_id: ProgramId,
    pub(crate) addresses: Vec<Address>,
    pub(crate) nonces: Vec<Nonce>,
    pub(crate) instruction_data: InstructionData,
}

impl Message {
    pub fn new<T: Serialize>(
        program_id: ProgramId,
        addresses: Vec<Address>,
        nonces: Vec<Nonce>,
        instruction: T,
    ) -> Self {
        let instruction_data = Program::serialize_instruction_data(instruction).unwrap();
        Self {
            program_id,
            addresses,
            nonces,
            instruction_data,
        }
    }
}

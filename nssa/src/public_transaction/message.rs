use nssa_core::{
    account::Nonce,
    program::{InstructionData, ProgramId},
};
use serde::{Deserialize, Serialize};

use crate::Address;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Message {
    pub(crate) program_id: ProgramId,
    pub(crate) addresses: Vec<Address>,
    pub(crate) nonces: Vec<Nonce>,
    pub(crate) instruction_data: InstructionData,
}

impl Message {
    pub fn new(
        program_id: ProgramId,
        addresses: Vec<Address>,
        nonces: Vec<Nonce>,
        instruction_data: InstructionData,
    ) -> Self {
        Self {
            program_id,
            addresses,
            nonces,
            instruction_data,
        }
    }
}

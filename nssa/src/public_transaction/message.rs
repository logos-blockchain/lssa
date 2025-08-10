use nssa_core::{account::Nonce, program::ProgramId};
use serde::{Deserialize, Serialize};

use crate::Address;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Message {
    pub(crate) program_id: ProgramId,
    pub(crate) addresses: Vec<Address>,
    pub(crate) nonces: Vec<Nonce>,
    // TODO: change to Vec<u8> for general programs
    pub(crate) instruction_data: u128,
}

impl Message {
    pub fn new(
        program_id: ProgramId,
        addresses: Vec<Address>,
        nonces: Vec<Nonce>,
        instruction_data: u128,
    ) -> Self {
        Self {
            program_id,
            addresses,
            nonces,
            instruction_data,
        }
    }
}

use std::io::{Cursor, Read};

use nssa_core::{
    account::Nonce,
    program::{InstructionData, ProgramId},
};
use serde::{Deserialize, Serialize};

use crate::{Address, error::NssaError, program::Program};
const MESSAGE_ENCODING_PREFIX_LEN: usize = 19;
const MESSAGE_ENCODING_PREFIX: &[u8; MESSAGE_ENCODING_PREFIX_LEN] = b"NSSA/v0.1/TxMessage";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

    /// Serializes a `Message` into bytes in the following layout:
    /// TAG || <program_id>  (bytes LE) * 8 || addresses_len (4 bytes LE) || addresses (32 bytes * N) || nonces_len (4 bytes LE) || nonces (16 bytes * M) || instruction_data_len || instruction_data (4 bytes * K)
    /// Integers and words are encoded in little-endian byte order, and fields appear in the above order.
    pub(crate) fn message_to_bytes(&self) -> Vec<u8> {
        let mut bytes = MESSAGE_ENCODING_PREFIX.to_vec();
        // program_id: [u32; 8]
        for word in &self.program_id {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        // addresses: Vec<[u8;32]>
        // serialize length as u32 little endian, then all addresses concatenated
        let addresses_len = self.addresses.len() as u32;
        bytes.extend(&addresses_len.to_le_bytes());
        for addr in &self.addresses {
            bytes.extend_from_slice(addr.value());
        }
        // nonces: Vec<u128>
        let nonces_len = self.nonces.len() as u32;
        bytes.extend(&nonces_len.to_le_bytes());
        for nonce in &self.nonces {
            bytes.extend(&nonce.to_le_bytes());
        }
        // instruction_data: Vec<u32>
        // serialize length as u32 little endian, then all addresses concatenated
        let instr_len = self.instruction_data.len() as u32;
        bytes.extend(&instr_len.to_le_bytes());
        for word in &self.instruction_data {
            bytes.extend(&word.to_le_bytes());
        }

        bytes
    }

    pub(crate) fn from_cursor(cursor: &mut Cursor<&[u8]>) -> Self {
        let prefix = {
            let mut this = [0u8; MESSAGE_ENCODING_PREFIX_LEN];
            cursor.read_exact(&mut this).unwrap();
            this
        };
        assert_eq!(&prefix, MESSAGE_ENCODING_PREFIX);
        let program_id: ProgramId = {
            let mut this = [0u32; 8];
            for i in 0..8 {
                this[i] = u32_from_cursor(cursor);
            }
            this
        };
        let addresses_len = u32_from_cursor(cursor);
        let mut addresses = Vec::with_capacity(addresses_len as usize);
        for _ in 0..addresses_len {
            let mut value = [0u8; 32];
            cursor.read_exact(&mut value).unwrap();
            addresses.push(Address::new(value))
        }
        let nonces_len = u32_from_cursor(cursor);
        let mut nonces = Vec::with_capacity(nonces_len as usize);
        for _ in 0..nonces_len {
            let mut buf = [0u8; 16];
            cursor.read_exact(&mut buf).unwrap();
            nonces.push(u128::from_le_bytes(buf))
        }
        let instruction_data_len = u32_from_cursor(cursor);
        let mut instruction_data = Vec::with_capacity(instruction_data_len as usize);
        for _ in 0..instruction_data_len {
            let word = u32_from_cursor(cursor);
            instruction_data.push(word)
        }
        Self {
            program_id,
            addresses,
            nonces,
            instruction_data,
        }
    }
}

fn u32_from_cursor(cursor: &mut Cursor<&[u8]>) -> u32 {
    let mut word_buf = [0u8; 4];
    cursor.read_exact(&mut word_buf).unwrap();
    u32::from_le_bytes(word_buf)
}

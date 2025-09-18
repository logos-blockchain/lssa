// TODO: Consider switching to deriving Borsh

use std::io::{Cursor, Read};

use nssa_core::program::ProgramId;

use crate::{
    Address, PublicKey, PublicTransaction, Signature,
    error::NssaError,
    public_transaction::{Message, WitnessSet},
};

const MESSAGE_ENCODING_PREFIX_LEN: usize = 22;
const MESSAGE_ENCODING_PREFIX: &[u8; MESSAGE_ENCODING_PREFIX_LEN] = b"\x00/NSSA/v0.1/TxMessage/";

impl Message {
    /// Serializes a `Message` into bytes in the following layout:
    /// PREFIX || <program_id>  (4 bytes LE) * 8 || addresses_len (4 bytes LE) || addresses (32 bytes * N) || nonces_len (4 bytes LE) || nonces (16 bytes LE * M) || instruction_data_len || instruction_data (4 bytes LE * K)
    /// Integers and words are encoded in little-endian byte order, and fields appear in the above order.
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
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
        // serialize length as u32 little endian, then all nonces concatenated in LE
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

    pub(crate) fn from_cursor(cursor: &mut Cursor<&[u8]>) -> Result<Self, NssaError> {
        let prefix = {
            let mut this = [0u8; MESSAGE_ENCODING_PREFIX_LEN];
            cursor.read_exact(&mut this)?;
            this
        };
        if &prefix != MESSAGE_ENCODING_PREFIX {
            return Err(NssaError::TransactionDeserializationError(
                "Invalid public message prefix".to_string(),
            ));
        }

        let program_id: ProgramId = {
            let mut this = [0u32; 8];
            for item in &mut this {
                *item = u32_from_cursor(cursor)?;
            }
            this
        };
        let addresses_len = u32_from_cursor(cursor)?;
        let mut addresses = Vec::with_capacity(addresses_len as usize);
        for _ in 0..addresses_len {
            let mut value = [0u8; 32];
            cursor.read_exact(&mut value)?;
            addresses.push(Address::new(value))
        }
        let nonces_len = u32_from_cursor(cursor)?;
        let mut nonces = Vec::with_capacity(nonces_len as usize);
        for _ in 0..nonces_len {
            let mut buf = [0u8; 16];
            cursor.read_exact(&mut buf)?;
            nonces.push(u128::from_le_bytes(buf))
        }
        let instruction_data_len = u32_from_cursor(cursor)?;
        let mut instruction_data = Vec::with_capacity(instruction_data_len as usize);
        for _ in 0..instruction_data_len {
            let word = u32_from_cursor(cursor)?;
            instruction_data.push(word)
        }
        Ok(Self {
            program_id,
            addresses,
            nonces,
            instruction_data,
        })
    }
}

impl WitnessSet {
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        let size = self.signatures_and_public_keys().len() as u32;
        bytes.extend_from_slice(&size.to_le_bytes());
        for (signature, public_key) in self.signatures_and_public_keys() {
            bytes.extend_from_slice(signature.to_bytes());
            bytes.extend_from_slice(public_key.to_bytes());
        }
        bytes
    }

    pub(crate) fn from_cursor(cursor: &mut Cursor<&[u8]>) -> Result<Self, NssaError> {
        let num_signatures: u32 = {
            let mut buf = [0u8; 4];
            cursor.read_exact(&mut buf)?;
            u32::from_le_bytes(buf)
        };
        let mut signatures_and_public_keys = Vec::with_capacity(num_signatures as usize);
        for _i in 0..num_signatures {
            let signature = Signature::from_cursor(cursor)?;
            let public_key = PublicKey::from_cursor(cursor)?;
            signatures_and_public_keys.push((signature, public_key))
        }
        Ok(Self {
            signatures_and_public_keys,
        })
    }
}

impl PublicTransaction {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = self.message().to_bytes();
        bytes.extend_from_slice(&self.witness_set().to_bytes());
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, NssaError> {
        let mut cursor = Cursor::new(bytes);
        Self::from_cursor(&mut cursor)
    }

    pub fn from_cursor(cursor: &mut Cursor<&[u8]>) -> Result<Self, NssaError> {
        let message = Message::from_cursor(cursor)?;
        let witness_set = WitnessSet::from_cursor(cursor)?;
        Ok(PublicTransaction::new(message, witness_set))
    }
}

fn u32_from_cursor(cursor: &mut Cursor<&[u8]>) -> Result<u32, NssaError> {
    let mut word_buf = [0u8; 4];
    cursor.read_exact(&mut word_buf)?;
    Ok(u32::from_le_bytes(word_buf))
}

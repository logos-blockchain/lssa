// TODO: Consider switching to deriving Borsh

use std::io::{Cursor, Read};

use crate::{
    ProgramDeploymentTransaction, error::NssaError, program_deployment_transaction::Message,
};

const MESSAGE_ENCODING_PREFIX_LEN: usize = 32;
const MESSAGE_ENCODING_PREFIX: &[u8; MESSAGE_ENCODING_PREFIX_LEN] =
    b"/NSSA/v0.2/TxMessage/Program/\x00\x00\x00";

impl Message {
    /// Serializes a `Message` into bytes in the following layout:
    /// PREFIX || bytecode_len  (4 bytes LE) || <bytecode>
    /// Integers are encoded in little-endian byte order, and fields appear in the above order.
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = MESSAGE_ENCODING_PREFIX.to_vec();
        let bytecode_len = self.bytecode.len() as u32;
        bytes.extend(&bytecode_len.to_le_bytes());
        bytes.extend(&self.bytecode);
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
        let bytecode_len = u32_from_cursor(cursor)?;
        let mut bytecode = vec![0; bytecode_len as usize];
        let num_bytes = cursor.read(&mut bytecode)?;
        if num_bytes != bytecode_len as usize {
            println!("num bytes: {}", num_bytes);
            return Err(NssaError::TransactionDeserializationError(
                "Invalid number of bytes".to_string(),
            ));
        }
        Ok(Self { bytecode })
    }
}

impl ProgramDeploymentTransaction {
    pub fn to_bytes(&self) -> Vec<u8> {
        self.message.to_bytes()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, NssaError> {
        let mut cursor = Cursor::new(bytes);
        Self::from_cursor(&mut cursor)
    }

    pub fn from_cursor(cursor: &mut Cursor<&[u8]>) -> Result<Self, NssaError> {
        let message = Message::from_cursor(cursor)?;
        Ok(Self::new(message))
    }
}

fn u32_from_cursor(cursor: &mut Cursor<&[u8]>) -> Result<u32, NssaError> {
    let mut word_buf = [0u8; 4];
    cursor.read_exact(&mut word_buf)?;
    Ok(u32::from_le_bytes(word_buf))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use crate::{ProgramDeploymentTransaction, program_deployment_transaction::Message};

    #[test]
    fn test_roundtrip() {
        let message = Message::new(vec![0xca, 0xfe, 0xca, 0xfe, 0x01, 0x02, 0x03]);
        let tx = ProgramDeploymentTransaction::new(message);
        let bytes = tx.to_bytes();
        let mut cursor = Cursor::new(bytes.as_ref());
        let tx_from_cursor = ProgramDeploymentTransaction::from_cursor(&mut cursor).unwrap();
        assert_eq!(tx, tx_from_cursor);
    }
}

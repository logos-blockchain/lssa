use rs_merkle::Hasher;
use std::io::{Cursor, Read};

use crate::{OwnHasher, transaction::EncodedTransaction};

pub type BlockHash = [u8; 32];
pub type BlockId = u64;
pub type TimeStamp = u64;

#[derive(Debug, Clone)]
pub struct BlockHeader {
    pub block_id: BlockId,
    pub prev_block_hash: BlockHash,
    pub hash: BlockHash,
    pub timestamp: TimeStamp,
    pub signature: nssa::Signature,
}

#[derive(Debug, Clone)]
pub struct BlockBody {
    pub transactions: Vec<EncodedTransaction>,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub header: BlockHeader,
    pub body: BlockBody,
}

#[derive(Debug, PartialEq, Eq)]
pub struct HashableBlockData {
    pub block_id: BlockId,
    pub prev_block_hash: BlockHash,
    pub timestamp: TimeStamp,
    pub transactions: Vec<EncodedTransaction>,
}

impl HashableBlockData {
    pub fn into_block(self, signing_key: &nssa::PrivateKey) -> Block {
        let data_bytes = self.to_bytes();
        let signature = nssa::Signature::new(signing_key, &data_bytes);
        let hash = OwnHasher::hash(&data_bytes);
        Block {
            header: BlockHeader {
                block_id: self.block_id,
                prev_block_hash: self.prev_block_hash,
                hash,
                timestamp: self.timestamp,
                signature,
            },
            body: BlockBody {
                transactions: self.transactions,
            },
        }
    }
}

impl From<Block> for HashableBlockData {
    fn from(value: Block) -> Self {
        Self {
            block_id: value.header.block_id,
            prev_block_hash: value.header.prev_block_hash,
            timestamp: value.header.timestamp,
            transactions: value.body.transactions,
        }
    }
}

impl HashableBlockData {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.block_id.to_le_bytes());
        bytes.extend_from_slice(&self.prev_block_hash);
        bytes.extend_from_slice(&self.timestamp.to_le_bytes());
        let num_transactions: u32 = self.transactions.len() as u32;
        bytes.extend_from_slice(&num_transactions.to_le_bytes());
        for tx in &self.transactions {
            let transaction_bytes = tx.to_bytes();
            let num_transaction_bytes: u32 = transaction_bytes.len() as u32;

            bytes.extend_from_slice(&num_transaction_bytes.to_le_bytes());
            bytes.extend_from_slice(&tx.to_bytes());
        }
        bytes
    }

    // TODO: Improve error handling. Remove unwraps.
    pub fn from_bytes(data: &[u8]) -> Self {
        let mut cursor = Cursor::new(data);

        let block_id = u64_from_cursor(&mut cursor);

        let mut prev_block_hash = [0u8; 32];
        cursor.read_exact(&mut prev_block_hash).unwrap();

        let timestamp = u64_from_cursor(&mut cursor);

        let num_transactions = u32_from_cursor(&mut cursor) as usize;

        let mut transactions = Vec::with_capacity(num_transactions);
        for _ in 0..num_transactions {
            let tx_len = u32_from_cursor(&mut cursor) as usize;
            let mut tx_bytes = Vec::with_capacity(tx_len);

            for _ in 0..tx_len {
                let mut buff = [0; 1];
                cursor.read_exact(&mut buff).unwrap();
                tx_bytes.push(buff[0]);
            }

            let tx = EncodedTransaction::from_bytes(tx_bytes);
            transactions.push(tx);
        }

        Self {
            block_id,
            prev_block_hash,
            timestamp,
            transactions,
        }
    }
}

// TODO: Improve error handling. Remove unwraps.
pub fn u32_from_cursor(cursor: &mut Cursor<&[u8]>) -> u32 {
    let mut word_buf = [0u8; 4];
    cursor.read_exact(&mut word_buf).unwrap();
    u32::from_le_bytes(word_buf)
}

// TODO: Improve error handling. Remove unwraps.
pub fn u64_from_cursor(cursor: &mut Cursor<&[u8]>) -> u64 {
    let mut word_buf = [0u8; 8];
    cursor.read_exact(&mut word_buf).unwrap();
    u64::from_le_bytes(word_buf)
}

#[cfg(test)]
mod tests {
    use crate::{block::HashableBlockData, test_utils};

    #[test]
    fn test_encoding_roundtrip() {
        let transactions = vec![test_utils::produce_dummy_empty_transaction()];
        let block = test_utils::produce_dummy_block(1, Some([1; 32]), transactions);
        let hashable = HashableBlockData::from(block);
        let bytes = hashable.to_bytes();
        let block_from_bytes = HashableBlockData::from_bytes(&bytes);
        assert_eq!(hashable, block_from_bytes);
    }
}

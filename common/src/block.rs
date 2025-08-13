use std::io::{Cursor, Read};

use rs_merkle::Hasher;

use crate::merkle_tree_public::hasher::OwnHasher;
use nssa;

pub type BlockHash = [u8; 32];
pub type BlockId = u64;

#[derive(Debug, Clone)]
pub struct Block {
    pub block_id: BlockId,
    pub prev_block_id: BlockId,
    pub prev_block_hash: BlockHash,
    pub hash: BlockHash,
    pub transactions: Vec<nssa::PublicTransaction>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct HashableBlockData {
    pub block_id: BlockId,
    pub prev_block_id: BlockId,
    pub prev_block_hash: BlockHash,
    pub transactions: Vec<nssa::PublicTransaction>,
}

impl From<HashableBlockData> for Block {
    fn from(value: HashableBlockData) -> Self {
        let data = value.to_bytes();
        let hash = OwnHasher::hash(&data);

        Self {
            block_id: value.block_id,
            prev_block_id: value.prev_block_id,
            hash,
            transactions: value.transactions,
            prev_block_hash: value.prev_block_hash,
        }
    }
}

impl From<Block> for HashableBlockData {
    fn from(value: Block) -> Self {
        Self {
            block_id: value.block_id,
            prev_block_id: value.prev_block_id,
            prev_block_hash: value.prev_block_hash,
            transactions: value.transactions,
        }
    }
}

impl HashableBlockData {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.block_id.to_le_bytes());
        bytes.extend_from_slice(&self.prev_block_id.to_le_bytes());
        bytes.extend_from_slice(&self.prev_block_hash);
        let num_transactions: u32 = self.transactions.len() as u32;
        bytes.extend_from_slice(&num_transactions.to_le_bytes());
        for tx in &self.transactions {
            bytes.extend_from_slice(&tx.to_bytes());
        }
        bytes
    }

    // TODO: Improve error handling. Remove unwraps.
    pub fn from_bytes(data: &[u8]) -> Self {
        let mut cursor = Cursor::new(data);

        let block_id = u64_from_cursor(&mut cursor);
        let prev_block_id = u64_from_cursor(&mut cursor);

        let mut prev_block_hash = [0u8; 32];
        cursor.read_exact(&mut prev_block_hash).unwrap();

        let num_transactions = u32_from_cursor(&mut cursor) as usize;

        let mut transactions = Vec::with_capacity(num_transactions);
        for _ in 0..num_transactions {
            let tx = nssa::PublicTransaction::from_cursor(&mut cursor).unwrap();
            transactions.push(tx);
        }

        Self {
            block_id,
            prev_block_id,
            prev_block_hash,
            transactions,
        }
    }
}

// TODO: Improve error handling. Remove unwraps.
fn u32_from_cursor(cursor: &mut Cursor<&[u8]>) -> u32 {
    let mut word_buf = [0u8; 4];
    cursor.read_exact(&mut word_buf).unwrap();
    u32::from_le_bytes(word_buf)
}

// TODO: Improve error handling. Remove unwraps.
fn u64_from_cursor(cursor: &mut Cursor<&[u8]>) -> u64 {
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

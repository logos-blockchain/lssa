use rs_merkle::Hasher;
use serde::{Deserialize, Serialize};

use crate::{merkle_tree_public::hasher::OwnHasher, transaction::Transaction};

pub type BlockHash = [u8; 32];
pub type Data = Vec<u8>;
pub type BlockId = u64;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Block {
    pub block_id: BlockId,
    pub prev_block_id: BlockId,
    pub prev_block_hash: BlockHash,
    pub hash: BlockHash,
    pub transactions: Vec<Transaction>,
    pub data: Data,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HashableBlockData {
    pub block_id: BlockId,
    pub prev_block_id: BlockId,
    pub prev_block_hash: BlockHash,
    pub transactions: Vec<Transaction>,
    pub data: Data,
}

impl Block {
    pub fn produce_block_from_hashable_data(hashable_data: HashableBlockData) -> Self {
        let data = serde_json::to_vec(&hashable_data).unwrap();

        let hash = OwnHasher::hash(&data);

        Self {
            block_id: hashable_data.block_id,
            prev_block_id: hashable_data.prev_block_id,
            hash,
            transactions: hashable_data.transactions,
            data: hashable_data.data,
            prev_block_hash: hashable_data.prev_block_hash,
        }
    }
}

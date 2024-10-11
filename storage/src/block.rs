use serde::{Deserialize, Serialize};

use crate::transaction::Transaction;

pub type BlockHash = [u8; 32];
pub type Data = Vec<u8>;
pub type BlockId = u64;

//ToDo: Add fields to block when model is clear
#[derive(Debug, Serialize, Deserialize)]
pub struct Block {
    pub block_id: BlockId,
    pub hash: BlockHash,
    pub transactions: Vec<Transaction>,
    pub data: Data,
}

use borsh::{BorshDeserialize, BorshSerialize};
use sha2::{Digest, Sha256, digest::FixedOutput};

use crate::transaction::EncodedTransaction;

pub type HashType = [u8; 32];

#[derive(Debug, Clone)]
///Our own hasher.
/// Currently it is SHA256 hasher wrapper. May change in a future.
pub struct OwnHasher {}

impl OwnHasher {
    fn hash(data: &[u8]) -> HashType {
        let mut hasher = Sha256::new();

        hasher.update(data);
        <HashType>::from(hasher.finalize_fixed())
    }
}

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

#[derive(Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct HashableBlockData {
    pub block_id: BlockId,
    pub prev_block_hash: BlockHash,
    pub timestamp: TimeStamp,
    pub transactions: Vec<EncodedTransaction>,
}

impl HashableBlockData {
    pub fn into_block(self, signing_key: &nssa::PrivateKey) -> Block {
        let data_bytes = borsh::to_vec(&self).unwrap();
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

#[cfg(test)]
mod tests {
    use crate::{block::HashableBlockData, test_utils};

    #[test]
    fn test_encoding_roundtrip() {
        let transactions = vec![test_utils::produce_dummy_empty_transaction()];
        let block = test_utils::produce_dummy_block(1, Some([1; 32]), transactions);
        let hashable = HashableBlockData::from(block);
        let bytes = borsh::to_vec(&hashable).unwrap();
        let block_from_bytes = borsh::from_slice::<HashableBlockData>(&bytes).unwrap();
        assert_eq!(hashable, block_from_bytes);
    }
}

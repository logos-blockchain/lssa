use borsh::{BorshDeserialize, BorshSerialize};
use logos_blockchain_core::mantle::ops::channel::MsgId;
use sha2::{Digest, Sha256, digest::FixedOutput};

use crate::transaction::EncodedTransaction;

pub type HashType = [u8; 32];

#[derive(Debug, Clone)]
/// Our own hasher.
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

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct BlockHeader {
    pub block_id: BlockId,
    pub prev_block_hash: BlockHash,
    pub hash: BlockHash,
    pub timestamp: TimeStamp,
    pub signature: nssa::Signature,
}

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct BlockBody {
    pub transactions: Vec<EncodedTransaction>,
}

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub enum BedrockStatus {
    Pending,
    Safe,
    Finalized,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct Block {
    pub header: BlockHeader,
    pub body: BlockBody,
    pub bedrock_status: BedrockStatus,
    #[borsh(
        serialize_with = "borsh_msg_id::serialize",
        deserialize_with = "borsh_msg_id::deserialize"
    )]
    pub bedrock_parent_id: MsgId,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct HashableBlockData {
    pub block_id: BlockId,
    pub prev_block_hash: BlockHash,
    pub timestamp: TimeStamp,
    pub transactions: Vec<EncodedTransaction>,
}

impl HashableBlockData {
    pub fn into_pending_block(
        self,
        signing_key: &nssa::PrivateKey,
        bedrock_parent_id: MsgId,
    ) -> Block {
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
            bedrock_status: BedrockStatus::Pending,
            bedrock_parent_id,
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

mod borsh_msg_id {
    use std::io::{Read, Write};

    use logos_blockchain_core::mantle::ops::channel::MsgId;

    pub fn serialize<W: Write>(v: &MsgId, w: &mut W) -> std::io::Result<()> {
        w.write_all(v.as_ref())
    }

    pub fn deserialize<R: Read>(r: &mut R) -> std::io::Result<MsgId> {
        let mut buf = [0u8; 32];
        r.read_exact(&mut buf)?;
        Ok(MsgId::from(buf))
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

use std::{collections::HashMap, path::Path};

use anyhow::Result;
use common::{
    HashType,
    block::{Block, BlockHash},
    transaction::EncodedTransaction,
};
use nssa::V02State;
use storage::RocksDBIO;

pub struct SequencerBlockStore {
    dbio: RocksDBIO,
    // TODO: Consider adding the hashmap to the database for faster recovery.
    tx_hash_to_block_map: HashMap<HashType, u64>,
    genesis_id: u64,
    signing_key: nssa::PrivateKey,
}

impl SequencerBlockStore {
    /// Starting database at the start of new chain.
    /// Creates files if necessary.
    ///
    /// ATTENTION: Will overwrite genesis block.
    pub fn open_db_with_genesis(
        location: &Path,
        genesis_block: Option<Block>,
        signing_key: nssa::PrivateKey,
    ) -> Result<Self> {
        let tx_hash_to_block_map = if let Some(block) = &genesis_block {
            block_to_transactions_map(block)
        } else {
            HashMap::new()
        };

        let dbio = RocksDBIO::open_or_create(location, genesis_block)?;

        let genesis_id = dbio.get_meta_first_block_in_db()?;

        Ok(Self {
            dbio,
            genesis_id,
            tx_hash_to_block_map,
            signing_key,
        })
    }

    /// Reopening existing database
    pub fn open_db_restart(location: &Path, signing_key: nssa::PrivateKey) -> Result<Self> {
        SequencerBlockStore::open_db_with_genesis(location, None, signing_key)
    }

    pub fn get_block_at_id(&self, id: u64) -> Result<Block> {
        Ok(self.dbio.get_block(id)?)
    }

    pub fn delete_block_at_id(&mut self, block_id: u64) -> Result<()> {
        Ok(self.dbio.delete_block(block_id)?)
    }

    /// Returns the transaction corresponding to the given hash, if it exists in the blockchain.
    pub fn get_transaction_by_hash(&self, hash: HashType) -> Option<EncodedTransaction> {
        let block_id = self.tx_hash_to_block_map.get(&hash);
        let block = block_id.map(|&id| self.get_block_at_id(id));
        if let Some(Ok(block)) = block {
            for transaction in block.body.transactions.into_iter() {
                if transaction.hash() == hash {
                    return Some(transaction);
                }
            }
        }
        None
    }

    pub fn insert(&mut self, tx: &EncodedTransaction, block_id: u64) {
        self.tx_hash_to_block_map.insert(tx.hash(), block_id);
    }

    pub fn genesis_id(&self) -> u64 {
        self.genesis_id
    }

    pub fn signing_key(&self) -> &nssa::PrivateKey {
        &self.signing_key
    }

    pub(crate) fn get_pending_blocks(&self) -> impl Iterator<Item = Result<Block>> {
        self.dbio.get_all_blocks().map(|res| Ok(res?))
    }

    pub(crate) fn update(&self, block: Block, state: &V02State) -> Result<()> {
        Ok(self.dbio.atomic_update(block, state)?)
    }
}

pub(crate) fn block_to_transactions_map(block: &Block) -> HashMap<HashType, u64> {
    block
        .body
        .transactions
        .iter()
        .map(|transaction| (transaction.hash(), block.header.block_id))
        .collect()
}

#[cfg(test)]
mod tests {
    use common::{block::HashableBlockData, test_utils::sequencer_sign_key_for_testing};
    use logos_blockchain_core::mantle::ops::channel::MsgId;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn test_get_transaction_by_hash() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path();

        let signing_key = sequencer_sign_key_for_testing();

        let genesis_block_hashable_data = HashableBlockData {
            block_id: 0,
            prev_block_hash: [0; 32],
            timestamp: 0,
            transactions: vec![],
        };

        let genesis_block =
            genesis_block_hashable_data.into_pending_block(&signing_key, MsgId::from([0; 32]));
        // Start an empty node store
        let mut node_store =
            SequencerBlockStore::open_db_with_genesis(path, Some(genesis_block), signing_key)
                .unwrap();

        let tx = common::test_utils::produce_dummy_empty_transaction();
        let block = common::test_utils::produce_dummy_block(1, None, vec![tx.clone()]);

        // Try retrieve a tx that's not in the chain yet.
        let retrieved_tx = node_store.get_transaction_by_hash(tx.hash());
        assert_eq!(None, retrieved_tx);
        // Add the block with the transaction
        let dummy_state = V02State::new_with_genesis_accounts(&[], &[]);
        node_store.update(block, &dummy_state).unwrap();
        // Try again
        let retrieved_tx = node_store.get_transaction_by_hash(tx.hash());
        assert_eq!(Some(tx), retrieved_tx);
    }
}

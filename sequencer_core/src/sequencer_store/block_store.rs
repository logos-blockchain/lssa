use std::{collections::HashMap, path::Path};

use anyhow::Result;
use common::{block::Block, merkle_tree_public::TreeHashType};
use storage::RocksDBIO;

pub struct SequecerBlockStore {
    dbio: RocksDBIO,
    // TODO: Consider adding the hashmap to the database for faster recovery.
    tx_hash_to_block_map: HashMap<TreeHashType, u64>,
    pub genesis_id: u64,
}

impl SequecerBlockStore {
    ///Starting database at the start of new chain.
    /// Creates files if necessary.
    ///
    /// ATTENTION: Will overwrite genesis block.
    pub fn open_db_with_genesis(location: &Path, genesis_block: Option<Block>) -> Result<Self> {
        let tx_hash_to_block_map = if let Some(block) = &genesis_block {
            block_to_transactions_map(block)
        } else {
            HashMap::new()
        };

        let dbio = RocksDBIO::new(location, genesis_block)?;

        let genesis_id = dbio.get_meta_first_block_in_db()?;

        Ok(Self {
            dbio,
            genesis_id,
            tx_hash_to_block_map,
        })
    }

    ///Reopening existing database
    pub fn open_db_restart(location: &Path) -> Result<Self> {
        SequecerBlockStore::open_db_with_genesis(location, None)
    }

    pub fn get_block_at_id(&self, id: u64) -> Result<Block> {
        Ok(self.dbio.get_block(id)?)
    }

    pub fn put_block_at_id(&mut self, block: Block) -> Result<()> {
        let new_transactions_map = block_to_transactions_map(&block);
        self.dbio.put_block(block, false)?;
        self.tx_hash_to_block_map.extend(new_transactions_map);
        Ok(())
    }

    /// Returns the transaction corresponding to the given hash, if it exists in the blockchain.
    pub fn get_transaction_by_hash(&self, hash: TreeHashType) -> Option<nssa::PublicTransaction> {
        let block_id = self.tx_hash_to_block_map.get(&hash);
        let block = block_id.map(|&id| self.get_block_at_id(id));
        if let Some(Ok(block)) = block {
            for transaction in block.transactions.into_iter() {
                if transaction.hash() == hash {
                    return Some(transaction);
                }
            }
        }
        None
    }
}

fn block_to_transactions_map(block: &Block) -> HashMap<TreeHashType, u64> {
    block
        .transactions
        .iter()
        .map(|transaction| (transaction.hash(), block.block_id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::tempdir;

    #[test]
    fn test_get_transaction_by_hash() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path();
        let genesis_block = Block {
            block_id: 0,
            prev_block_id: 0,
            prev_block_hash: [0; 32],
            hash: [1; 32],
            transactions: vec![],
        };
        // Start an empty node store
        let mut node_store =
            SequecerBlockStore::open_db_with_genesis(path, Some(genesis_block)).unwrap();

        let tx = common::test_utils::produce_dummy_empty_transaction();
        let block = common::test_utils::produce_dummy_block(1, None, vec![tx.clone()]);

        // Try retrieve a tx that's not in the chain yet.
        let retrieved_tx = node_store.get_transaction_by_hash(tx.hash());
        assert_eq!(None, retrieved_tx);
        // Add the block with the transaction
        node_store.put_block_at_id(block).unwrap();
        // Try again
        let retrieved_tx = node_store.get_transaction_by_hash(tx.hash());
        assert_eq!(Some(tx), retrieved_tx);
    }
}

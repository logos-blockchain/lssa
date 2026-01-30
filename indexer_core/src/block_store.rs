use std::path::Path;

use anyhow::Result;
use common::{block::Block, transaction::{NSSATransaction, execute_check_transaction_on_state}};
use nssa::V02State;
use storage::indexer::RocksDBIO;

pub struct IndexerStore {
    dbio: RocksDBIO,
}

impl IndexerStore {
    /// Starting database at the start of new chain.
    /// Creates files if necessary.
    ///
    /// ATTENTION: Will overwrite genesis block.
    pub fn open_db_with_genesis(
        location: &Path,
        start_data: Option<(Block, V02State)>,
    ) -> Result<Self> {
        let dbio = RocksDBIO::open_or_create(location, start_data)?;

        Ok(Self {
            dbio,
        })
    }

    /// Reopening existing database
    pub fn open_db_restart(location: &Path) -> Result<Self> {
        Self::open_db_with_genesis(location, None)
    }

    pub fn get_block_at_id(&self, id: u64) -> Result<Block> {
        Ok(self.dbio.get_block(id)?)
    }

    pub fn genesis_id(&self) -> u64 {
        self.dbio.get_meta_first_block_in_db().expect("Must be set at the DB startup")
    }

    pub fn last_block(&self) -> u64 {
        self.dbio.get_meta_last_block_in_db().expect("Must be set at the DB startup")
    }

    pub fn get_state_at_block(&self, block_id: u64) -> Result<V02State> {
        Ok(self.dbio.calculate_state_for_id(block_id)?)
    }

    pub fn put_block(&self, block: Block) -> Result<()> {
        let mut final_state = self.dbio.final_state()?;

        for encoded_transaction in &block.body.transactions {
            let transaction = NSSATransaction::try_from(encoded_transaction)?;
            execute_check_transaction_on_state(&mut final_state, transaction)?;
        }

        Ok(self.dbio.put_block(block)?)
    }
}

// #[cfg(test)]
// mod tests {
//     use common::{block::HashableBlockData, test_utils::sequencer_sign_key_for_testing};
//     use tempfile::tempdir;

//     use super::*;

//     #[test]
//     fn test_get_transaction_by_hash() {
//         let temp_dir = tempdir().unwrap();
//         let path = temp_dir.path();

//         let signing_key = sequencer_sign_key_for_testing();

//         let genesis_block_hashable_data = HashableBlockData {
//             block_id: 0,
//             prev_block_hash: [0; 32],
//             timestamp: 0,
//             transactions: vec![],
//         };

//         let genesis_block = genesis_block_hashable_data.into_pending_block(&signing_key, [0; 32]);
//         // Start an empty node store
//         let mut node_store =
//             SequencerStore::open_db_with_genesis(path, Some(genesis_block), signing_key).unwrap();

//         let tx = common::test_utils::produce_dummy_empty_transaction();
//         let block = common::test_utils::produce_dummy_block(1, None, vec![tx.clone()]);

//         // Try retrieve a tx that's not in the chain yet.
//         let retrieved_tx = node_store.get_transaction_by_hash(tx.hash());
//         assert_eq!(None, retrieved_tx);
//         // Add the block with the transaction
//         let dummy_state = V02State::new_with_genesis_accounts(&[], &[]);
//         node_store.update(block, &dummy_state).unwrap();
//         // Try again
//         let retrieved_tx = node_store.get_transaction_by_hash(tx.hash());
//         assert_eq!(Some(tx), retrieved_tx);
//     }
// }

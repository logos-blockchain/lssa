use std::{path::Path, sync::Arc};

use anyhow::Result;
use common::{
    block::Block,
    transaction::{NSSATransaction, execute_check_transaction_on_state, transaction_pre_check},
};
use nssa::V02State;
use storage::indexer::RocksDBIO;

#[derive(Clone)]
pub struct IndexerStore {
    dbio: Arc<RocksDBIO>,
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

        Ok(Self { dbio: Arc::new(dbio) })
    }

    /// Reopening existing database
    pub fn open_db_restart(location: &Path) -> Result<Self> {
        Self::open_db_with_genesis(location, None)
    }

    pub fn get_block_at_id(&self, id: u64) -> Result<Block> {
        Ok(self.dbio.get_block(id)?)
    }

    pub fn genesis_id(&self) -> u64 {
        self.dbio
            .get_meta_first_block_in_db()
            .expect("Must be set at the DB startup")
    }

    pub fn last_block(&self) -> u64 {
        self.dbio
            .get_meta_last_block_in_db()
            .expect("Must be set at the DB startup")
    }

    pub fn get_state_at_block(&self, block_id: u64) -> Result<V02State> {
        Ok(self.dbio.calculate_state_for_id(block_id)?)
    }

    pub fn final_state(&self) -> Result<V02State> {
        Ok(self.dbio.final_state()?)
    }

    pub fn put_block(&self, block: Block) -> Result<()> {
        let mut final_state = self.dbio.final_state()?;

        for encoded_transaction in &block.body.transactions {
            let transaction = NSSATransaction::try_from(encoded_transaction)?;
            execute_check_transaction_on_state(
                &mut final_state,
                transaction_pre_check(transaction)?,
            )?;
        }

        Ok(self.dbio.put_block(block)?)
    }
}

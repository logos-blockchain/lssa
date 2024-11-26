use std::path::Path;

use anyhow::Result;
use storage::{block::Block, RocksDBIO};

pub struct SequecerBlockStore {
    dbio: RocksDBIO,
}

impl SequecerBlockStore {
    ///Starting database at the start of new chain.
    /// Creates files if necessary.
    ///
    /// ATTENTION: Will overwrite genesis block.
    pub fn open_db_with_genesis(location: &Path, genesis_block: Option<Block>) -> Result<Self> {
        Ok(Self {
            dbio: RocksDBIO::new(location, genesis_block)?,
        })
    }

    ///Reopening existing database
    pub fn open_db_restart(location: &Path) -> Result<Self> {
        SequecerBlockStore::open_db_with_genesis(location, None)
    }

    pub fn get_block_at_id(&self, id: u64) -> Result<Block> {
        Ok(self.dbio.get_block(id)?)
    }

    pub fn put_block_at_id(&self, block: Block) -> Result<()> {
        Ok(self.dbio.put_block(block)?)
    }
}

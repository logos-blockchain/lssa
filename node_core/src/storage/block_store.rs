use std::path::Path;

use anyhow::{anyhow, Result};
use common::block::Block;
use storage::sc_db_utils::DataBlob;
use storage::RocksDBIO;

pub struct NodeBlockStore {
    dbio: RocksDBIO,
}

impl NodeBlockStore {
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
        NodeBlockStore::db_destroy(location)?;
        NodeBlockStore::open_db_with_genesis(location, None)
    }

    ///Reloading existing database
    pub fn open_db_reload(location: &Path) -> Result<Self> {
        NodeBlockStore::open_db_with_genesis(location, None)
    }

    ///Destroying existing database
    fn db_destroy(location: &Path) -> Result<()> {
        RocksDBIO::destroy(location).map_err(|err| anyhow!("RocksDBIO error: {}", err))
    }

    pub fn get_block_at_id(&self, id: u64) -> Result<Block> {
        Ok(self.dbio.get_block(id)?)
    }

    pub fn put_block_at_id(&self, block: Block) -> Result<()> {
        Ok(self.dbio.put_block(block, false)?)
    }

    pub fn get_sc_sc_state(&self, sc_addr: &str) -> Result<Vec<DataBlob>> {
        Ok(self.dbio.get_sc_sc_state(sc_addr)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::block::Data;
    use tempfile::tempdir;

    fn create_genesis_block() -> Block {
        Block {
            block_id: 0,
            prev_block_id: 0,
            prev_block_hash: [0; 32],
            hash: [1; 32],
            transactions: vec![],
            data: Data::default(),
        }
    }

    fn create_sample_block(block_id: u64, prev_block_id: u64) -> Block {
        Block {
            block_id: block_id,
            prev_block_id: prev_block_id,
            prev_block_hash: [0; 32],
            hash: [1; 32],
            transactions: vec![],
            data: Data::default(),
        }
    }

    #[test]
    fn test_open_db_with_genesis() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path();

        let genesis_block = create_genesis_block();
        let node_store =
            NodeBlockStore::open_db_with_genesis(path, Some(genesis_block.clone())).unwrap();

        // Verify the genesis block is stored
        let stored_block = node_store.get_block_at_id(0).unwrap();
        assert_eq!(stored_block.block_id, genesis_block.block_id);
        assert_eq!(stored_block.hash, genesis_block.hash);
    }

    #[test]
    fn test_open_db_restart() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path();

        let genesis_block = create_genesis_block();
        let _ = NodeBlockStore::open_db_with_genesis(path, Some(genesis_block)).unwrap();

        // Restart the database
        let node_store = NodeBlockStore::open_db_restart(path).unwrap();

        // The block should no longer be available since no genesis block is set on restart
        let result = node_store.get_block_at_id(0);
        assert!(result.is_err());
    }

    #[test]
    fn test_open_db_reload() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path();

        let genesis_block = create_genesis_block();
        let _ = NodeBlockStore::open_db_with_genesis(path, Some(genesis_block)).unwrap();

        // Reload the database
        let node_store = NodeBlockStore::open_db_reload(path).unwrap();

        // The genesis block should be available on reload
        let result = node_store.get_block_at_id(0);
        assert!(!result.is_err());
    }

    #[test]
    fn test_put_and_get_block() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path();

        let genesis_block = create_genesis_block();
        let node_store = NodeBlockStore::open_db_with_genesis(path, Some(genesis_block)).unwrap();

        let block = create_sample_block(1, 0);
        node_store.put_block_at_id(block.clone()).unwrap();

        let retrieved_block = node_store.get_block_at_id(1).unwrap();
        assert_eq!(retrieved_block.block_id, block.block_id);
        assert_eq!(retrieved_block.hash, block.hash);
    }

    #[test]
    fn test_get_block_not_found() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path();

        let node_store = NodeBlockStore::open_db_with_genesis(path, None).unwrap();

        let result = node_store.get_block_at_id(42);
        assert!(result.is_err());
    }
}

use std::collections::{HashMap, HashSet};
use std::path::Path;

use accounts::account_core::Account;
use anyhow::{anyhow, Result};
use common::block::Block;
use common::merkle_tree_public::merkle_tree::HashStorageMerkleTree;
use common::nullifier::UTXONullifier;
use common::transaction::Transaction;
use common::utxo_commitment::UTXOCommitment;
use log::error;
use storage::sc_db_utils::{DataBlob, DataBlobChangeVariant};
use storage::RocksDBIO;

use crate::chain_storage::AccMap;

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
    pub fn open_db_restart(location: &Path, genesis_block: Block) -> Result<Self> {
        NodeBlockStore::db_destroy(location)?;
        NodeBlockStore::open_db_with_genesis(location, Some(genesis_block))
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

    pub fn put_sc_sc_state(
        &self,
        sc_addr: &str,
        length: usize,
        modifications: Vec<DataBlobChangeVariant>,
    ) -> Result<()> {
        Ok(self.dbio.put_sc_sc_state(sc_addr, length, modifications)?)
    }

    pub fn get_sc_sc_state(&self, sc_addr: &str) -> Result<Vec<DataBlob>> {
        Ok(self.dbio.get_sc_sc_state(sc_addr)?)
    }

    pub fn get_snapshot_block_id(&self) -> Result<u64> {
        Ok(self.dbio.get_snapshot_block_id()?)
    }

    pub fn get_snapshot_account(&self) -> Result<HashMap<[u8; 32], Account>> {
        let temp: AccMap = serde_json::from_slice(&self.dbio.get_snapshot_account()?)?;
        Ok(temp.into())
    }

    pub fn get_snapshot_commitment(&self) -> Result<HashStorageMerkleTree<UTXOCommitment>> {
        Ok(serde_json::from_slice(
            &self.dbio.get_snapshot_commitment()?,
        )?)
    }

    pub fn get_snapshot_nullifier(&self) -> Result<HashSet<UTXONullifier>> {
        Ok(serde_json::from_slice(
            &self.dbio.get_snapshot_nullifier()?,
        )?)
    }

    pub fn get_snapshot_transaction(&self) -> Result<HashStorageMerkleTree<Transaction>> {
        Ok(serde_json::from_slice(
            &self.dbio.get_snapshot_transaction()?,
        )?)
    }

    pub fn put_snapshot_at_block_id(
        &self,
        id: u64,
        accounts_ser: Vec<u8>,
        comm_ser: Vec<u8>,
        txs_ser: Vec<u8>,
        nullifiers_ser: Vec<u8>,
    ) -> Result<()> {
        //Error notification for writing into DB error
        self.dbio
            .put_snapshot_block_id_db(id)
            .inspect_err(|err| error!("Failed to store snapshot block id with error {err:#?}"))?;
        self.dbio
            .put_snapshot_account_db(accounts_ser)
            .inspect_err(|err| error!("Failed to store snapshot accounts with error {err:#?}"))?;
        self.dbio
            .put_snapshot_commitement_db(comm_ser)
            .inspect_err(|err| {
                error!("Failed to store snapshot commitments with error {err:#?}")
            })?;
        self.dbio
            .put_snapshot_transaction_db(txs_ser)
            .inspect_err(|err| {
                error!("Failed to store snapshot transactions with error {err:#?}")
            })?;
        self.dbio
            .put_snapshot_nullifier_db(nullifiers_ser)
            .inspect_err(|err| error!("Failed to store snapshot nullifiers with error {err:#?}"))?;

        Ok(())
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
            block_id,
            prev_block_id,
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
        {
            let node_store_old =
                NodeBlockStore::open_db_with_genesis(path, Some(genesis_block.clone())).unwrap();

            let block = create_sample_block(1, 0);
            node_store_old.put_block_at_id(block.clone()).unwrap();
        }

        // Check that the first block is still in the old database
        {
            let node_store_old = NodeBlockStore::open_db_reload(path).unwrap();
            let result = node_store_old.get_block_at_id(1);
            assert!(result.is_ok());
        }

        // Restart the database
        let node_store = NodeBlockStore::open_db_restart(path, genesis_block).unwrap();

        // The block should no longer be available since no first block is set on restart
        let result = node_store.get_block_at_id(1);
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
        assert!(result.is_ok());
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
    fn test_put_snapshot_at_block_id() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path();

        let genesis_block = create_genesis_block();
        let node_store = NodeBlockStore::open_db_with_genesis(path, Some(genesis_block)).unwrap();

        let id = 3;
        let accounts_ser = vec![1, 2, 3, 4];
        let comm_ser = vec![5, 6, 7, 8];
        let txs_ser = vec![9, 10, 11, 12];
        let nullifiers_ser = vec![13, 14, 15, 16];

        node_store
            .put_snapshot_at_block_id(
                id,
                accounts_ser.clone(),
                comm_ser.clone(),
                txs_ser.clone(),
                nullifiers_ser.clone(),
            )
            .unwrap();

        assert_eq!(node_store.dbio.get_snapshot_block_id().unwrap(), id);
        assert_eq!(
            node_store.dbio.get_snapshot_account().unwrap(),
            accounts_ser
        );
        assert_eq!(node_store.dbio.get_snapshot_commitment().unwrap(), comm_ser);
        assert_eq!(node_store.dbio.get_snapshot_transaction().unwrap(), txs_ser);
        assert_eq!(
            node_store.dbio.get_snapshot_nullifier().unwrap(),
            nullifiers_ser
        );
    }
}

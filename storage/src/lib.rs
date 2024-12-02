use std::{path::Path, sync::Arc};

use block::Block;
use error::DbError;
use log::warn;
use merkle_tree_public::TreeHashType;
use rocksdb::{
    BoundColumnFamily, ColumnFamilyDescriptor, DBWithThreadMode, MultiThreaded, Options,
};

pub mod block;
pub mod commitment;
pub mod commitments_sparse_merkle_tree;
pub mod error;
pub mod merkle_tree_public;
pub mod nullifier;
pub mod nullifier_sparse_merkle_tree;
pub mod transaction;
pub mod utxo_commitment;

///Account id on blockchain
pub type AccountId = TreeHashType;

///Maximal size of stored blocks in base
///
///Used to control db size
///
///Currently effectively unbounded.
pub const BUFF_SIZE_ROCKSDB: usize = usize::MAX;

///Size of stored blocks cache in memory
///
///Keeping small to not run out of memory
pub const CACHE_SIZE: usize = 1000;

///Key base for storing metainformation about id of first block in db
pub const DB_META_FIRST_BLOCK_IN_DB_KEY: &str = "first_block_in_db";
///Key base for storing metainformation about id of last current block in db
pub const DB_META_LAST_BLOCK_IN_DB_KEY: &str = "last_block_in_db";
///Key base for storing metainformation which describe if first block has been set
pub const DB_META_FIRST_BLOCK_SET_KEY: &str = "first_block_set";

///Name of block column family
pub const CF_BLOCK_NAME: &str = "cf_block";
///Name of meta column family
pub const CF_META_NAME: &str = "cf_meta";

pub type DbResult<T> = Result<T, DbError>;

pub struct RocksDBIO {
    pub db: DBWithThreadMode<MultiThreaded>,
}

impl RocksDBIO {
    pub fn new(path: &Path, start_block: Option<Block>) -> DbResult<Self> {
        let mut cf_opts = Options::default();
        cf_opts.set_max_write_buffer_number(16);
        //ToDo: Add more column families for different data
        let cfb = ColumnFamilyDescriptor::new(CF_BLOCK_NAME, cf_opts.clone());
        let cfmeta = ColumnFamilyDescriptor::new(CF_META_NAME, cf_opts.clone());

        let mut db_opts = Options::default();
        db_opts.create_missing_column_families(true);
        db_opts.create_if_missing(true);
        let db = DBWithThreadMode::<MultiThreaded>::open_cf_descriptors(
            &db_opts,
            path,
            vec![cfb, cfmeta],
        );

        let dbio = Self {
            //There is no point in handling this from runner code
            db: db.unwrap(),
        };

        let is_start_set = dbio.get_meta_is_first_block_set()?;

        if is_start_set {
            Ok(dbio)
        } else if let Some(block) = start_block {
            dbio.put_meta_first_block_in_db(block)?;
            dbio.put_meta_is_first_block_set()?;

            Ok(dbio)
        } else {
            warn!("Starting db in unset mode, will have to set starting block manually");

            Ok(dbio)
        }
    }

    pub fn meta_column(&self) -> Arc<BoundColumnFamily> {
        self.db.cf_handle(CF_META_NAME).unwrap()
    }

    pub fn block_column(&self) -> Arc<BoundColumnFamily> {
        self.db.cf_handle(CF_BLOCK_NAME).unwrap()
    }

    pub fn get_meta_first_block_in_db(&self) -> DbResult<u64> {
        let cf_meta = self.meta_column();
        let res = self
            .db
            .get_cf(&cf_meta, DB_META_FIRST_BLOCK_IN_DB_KEY)
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        if let Some(data) = res {
            Ok(u64::from_be_bytes(data.try_into().unwrap()))
        } else {
            Err(DbError::db_interaction_error(
                "First block not found".to_string(),
            ))
        }
    }

    pub fn get_meta_last_block_in_db(&self) -> DbResult<u64> {
        let cf_meta = self.meta_column();
        let res = self
            .db
            .get_cf(&cf_meta, DB_META_LAST_BLOCK_IN_DB_KEY)
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        if let Some(data) = res {
            Ok(u64::from_be_bytes(data.try_into().unwrap()))
        } else {
            Err(DbError::db_interaction_error(
                "Last block not found".to_string(),
            ))
        }
    }

    pub fn get_meta_is_first_block_set(&self) -> DbResult<bool> {
        let cf_meta = self.meta_column();
        let res = self
            .db
            .get_cf(&cf_meta, DB_META_FIRST_BLOCK_SET_KEY)
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        Ok(res.is_some())
    }

    pub fn put_meta_first_block_in_db(&self, block: Block) -> DbResult<()> {
        let cf_meta = self.meta_column();
        self.db
            .put_cf(
                &cf_meta,
                DB_META_FIRST_BLOCK_IN_DB_KEY.as_bytes(),
                block.block_id.to_be_bytes(),
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        self.put_block(block)?;
        Ok(())
    }

    pub fn put_meta_last_block_in_db(&self, block_id: u64) -> DbResult<()> {
        let cf_meta = self.meta_column();
        self.db
            .put_cf(
                &cf_meta,
                DB_META_LAST_BLOCK_IN_DB_KEY.as_bytes(),
                block_id.to_be_bytes(),
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;
        Ok(())
    }

    pub fn put_meta_is_first_block_set(&self) -> DbResult<()> {
        let cf_meta = self.meta_column();
        self.db
            .put_cf(&cf_meta, DB_META_FIRST_BLOCK_SET_KEY.as_bytes(), [1u8; 1])
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;
        Ok(())
    }

    pub fn put_block(&self, block: Block) -> DbResult<()> {
        let cf_block = self.block_column();

        let last_curr_block = self.get_meta_last_block_in_db()?;

        if block.block_id > last_curr_block {
            self.put_meta_last_block_in_db(block.block_id)?;
        }

        self.db
            .put_cf(
                &cf_block,
                block.block_id.to_be_bytes(),
                serde_json::to_vec(&block).map_err(|serr| {
                    DbError::serde_cast_message(
                        serr,
                        Some("Block Serialization failed".to_string()),
                    )
                })?,
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;
        Ok(())
    }

    pub fn get_block(&self, block_id: u64) -> DbResult<Block> {
        let cf_block = self.block_column();
        let res = self
            .db
            .get_cf(&cf_block, block_id.to_be_bytes())
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        if let Some(data) = res {
            Ok(serde_json::from_slice::<Block>(&data).map_err(|serr| {
                DbError::serde_cast_message(serr, Some("Block Deserialization failed".to_string()))
            })?)
        } else {
            Err(DbError::db_interaction_error(
                "Block on this id not found".to_string(),
            ))
        }
    }
}

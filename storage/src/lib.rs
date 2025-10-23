use std::{path::Path, sync::Arc};

use common::block::{Block, HashableBlockData};
use error::DbError;
use rocksdb::{
    BoundColumnFamily, ColumnFamilyDescriptor, DBWithThreadMode, MultiThreaded, Options,
};

pub mod error;

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

///Key base for storing snapshot which describe block id
pub const DB_SNAPSHOT_BLOCK_ID_KEY: &str = "block_id";

///Name of block column family
pub const CF_BLOCK_NAME: &str = "cf_block";
///Name of meta column family
pub const CF_META_NAME: &str = "cf_meta";
///Name of snapshot column family
pub const CF_SNAPSHOT_NAME: &str = "cf_snapshot";

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
        let cfsnapshot = ColumnFamilyDescriptor::new(CF_SNAPSHOT_NAME, cf_opts.clone());

        let mut db_opts = Options::default();
        db_opts.create_missing_column_families(true);
        db_opts.create_if_missing(true);
        let db = DBWithThreadMode::<MultiThreaded>::open_cf_descriptors(
            &db_opts,
            path,
            vec![cfb, cfmeta, cfsnapshot],
        );

        let dbio = Self {
            //There is no point in handling this from runner code
            db: db.unwrap(),
        };

        let is_start_set = dbio.get_meta_is_first_block_set()?;

        if is_start_set {
            Ok(dbio)
        } else if let Some(block) = start_block {
            let block_id = block.header.block_id;
            dbio.put_meta_first_block_in_db(block)?;
            dbio.put_meta_is_first_block_set()?;

            dbio.put_meta_last_block_in_db(block_id)?;

            Ok(dbio)
        } else {
            // Here we are trying to start a DB without a block, one should not do it.
            unreachable!()
        }
    }

    pub fn destroy(path: &Path) -> DbResult<()> {
        let mut cf_opts = Options::default();
        cf_opts.set_max_write_buffer_number(16);
        //ToDo: Add more column families for different data
        let _cfb = ColumnFamilyDescriptor::new(CF_BLOCK_NAME, cf_opts.clone());
        let _cfmeta = ColumnFamilyDescriptor::new(CF_META_NAME, cf_opts.clone());
        let _cfsnapshot = ColumnFamilyDescriptor::new(CF_SNAPSHOT_NAME, cf_opts.clone());

        let mut db_opts = Options::default();
        db_opts.create_missing_column_families(true);
        db_opts.create_if_missing(true);
        DBWithThreadMode::<MultiThreaded>::destroy(&db_opts, path)
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))
    }

    pub fn meta_column(&self) -> Arc<BoundColumnFamily<'_>> {
        self.db.cf_handle(CF_META_NAME).unwrap()
    }

    pub fn block_column(&self) -> Arc<BoundColumnFamily<'_>> {
        self.db.cf_handle(CF_BLOCK_NAME).unwrap()
    }

    pub fn snapshot_column(&self) -> Arc<BoundColumnFamily<'_>> {
        self.db.cf_handle(CF_SNAPSHOT_NAME).unwrap()
    }

    pub fn get_meta_first_block_in_db(&self) -> DbResult<u64> {
        let cf_meta = self.meta_column();
        let res = self
            .db
            .get_cf(
                &cf_meta,
                borsh::to_vec(&DB_META_FIRST_BLOCK_IN_DB_KEY).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize DB_META_FIRST_BLOCK_IN_DB_KEY".to_string()),
                    )
                })?,
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        if let Some(data) = res {
            Ok(borsh::from_slice::<u64>(&data).map_err(|err| {
                DbError::borsh_cast_message(
                    err,
                    Some("Failed to deserialize first block".to_string()),
                )
            })?)
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
            .get_cf(
                &cf_meta,
                borsh::to_vec(&DB_META_LAST_BLOCK_IN_DB_KEY).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize DB_META_LAST_BLOCK_IN_DB_KEY".to_string()),
                    )
                })?,
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        if let Some(data) = res {
            Ok(borsh::from_slice::<u64>(&data).map_err(|err| {
                DbError::borsh_cast_message(
                    err,
                    Some("Failed to deserialize last block".to_string()),
                )
            })?)
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
            .get_cf(
                &cf_meta,
                borsh::to_vec(&DB_META_FIRST_BLOCK_SET_KEY).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize DB_META_FIRST_BLOCK_SET_KEY".to_string()),
                    )
                })?,
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        Ok(res.is_some())
    }

    pub fn put_meta_first_block_in_db(&self, block: Block) -> DbResult<()> {
        let cf_meta = self.meta_column();
        self.db
            .put_cf(
                &cf_meta,
                borsh::to_vec(&DB_META_FIRST_BLOCK_IN_DB_KEY).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize DB_META_FIRST_BLOCK_IN_DB_KEY".to_string()),
                    )
                })?,
                borsh::to_vec(&block.header.block_id).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize first block id".to_string()),
                    )
                })?,
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        self.put_block(block, true)?;
        Ok(())
    }

    pub fn put_meta_last_block_in_db(&self, block_id: u64) -> DbResult<()> {
        let cf_meta = self.meta_column();
        self.db
            .put_cf(
                &cf_meta,
                borsh::to_vec(&DB_META_LAST_BLOCK_IN_DB_KEY).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize DB_META_LAST_BLOCK_IN_DB_KEY".to_string()),
                    )
                })?,
                borsh::to_vec(&block_id).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize last block id".to_string()),
                    )
                })?,
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;
        Ok(())
    }

    pub fn put_meta_is_first_block_set(&self) -> DbResult<()> {
        let cf_meta = self.meta_column();
        self.db
            .put_cf(
                &cf_meta,
                borsh::to_vec(&DB_META_FIRST_BLOCK_SET_KEY).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize DB_META_FIRST_BLOCK_SET_KEY".to_string()),
                    )
                })?,
                [1u8; 1],
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;
        Ok(())
    }

    pub fn put_block(&self, block: Block, first: bool) -> DbResult<()> {
        let cf_block = self.block_column();

        if !first {
            let last_curr_block = self.get_meta_last_block_in_db()?;

            if block.header.block_id > last_curr_block {
                self.put_meta_last_block_in_db(block.header.block_id)?;
            }
        }

        self.db
            .put_cf(
                &cf_block,
                borsh::to_vec(&block.header.block_id).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize block id".to_string()),
                    )
                })?,
                borsh::to_vec(&HashableBlockData::from(block)).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize block data".to_string()),
                    )
                })?,
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;
        Ok(())
    }

    pub fn get_block(&self, block_id: u64) -> DbResult<HashableBlockData> {
        let cf_block = self.block_column();
        let res = self
            .db
            .get_cf(
                &cf_block,
                borsh::to_vec(&block_id).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize block id".to_string()),
                    )
                })?,
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        if let Some(data) = res {
            Ok(
                borsh::from_slice::<HashableBlockData>(&data).map_err(|serr| {
                    DbError::borsh_cast_message(
                        serr,
                        Some("Failed to deserialize block data".to_string()),
                    )
                })?,
            )
        } else {
            Err(DbError::db_interaction_error(
                "Block on this id not found".to_string(),
            ))
        }
    }

    pub fn get_snapshot_block_id(&self) -> DbResult<u64> {
        let cf_snapshot = self.snapshot_column();
        let res = self
            .db
            .get_cf(
                &cf_snapshot,
                borsh::to_vec(&DB_SNAPSHOT_BLOCK_ID_KEY).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize DB_SNAPSHOT_BLOCK_ID_KEY".to_string()),
                    )
                })?,
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        if let Some(data) = res {
            Ok(borsh::from_slice::<u64>(&data).map_err(|err| {
                DbError::borsh_cast_message(
                    err,
                    Some("Failed to deserialize last block".to_string()),
                )
            })?)
        } else {
            Err(DbError::db_interaction_error(
                "Snapshot block ID not found".to_string(),
            ))
        }
    }
}

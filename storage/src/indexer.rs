use std::{path::Path, sync::Arc};

use common::block::{Block, HashableBlockData};
use rocksdb::{BoundColumnFamily, ColumnFamilyDescriptor, DBWithThreadMode, MultiThreaded, Options};

use crate::error::DbError;

/// Maximal size of stored diff in base
///
/// Used to control db size
///
/// Currently effectively unbounded.
pub const BUFF_SIZE_ROCKSDB: usize = usize::MAX;

/// Delay in diffs between breakpoints 
pub const BREAKPOINT_DELAY: usize = 100;

/// Key base for storing metainformation about id of first diff in db
pub const DB_META_FIRST_DIFF_IN_DB_KEY: &str = "first_diff_in_db";
/// Key base for storing metainformation about id of last current diff in db
pub const DB_META_LAST_DIFF_IN_DB_KEY: &str = "last_diff_in_db";
/// Key base for storing metainformation which describe if first diff has been set
pub const DB_META_FIRST_DIFF_SET_KEY: &str = "first_diff_set";

/// Name of diff column family
pub const CF_DIFF_NAME: &str = "cf_diff";
/// Name of breakpoint coumn family
pub const CF_BREAKPOINT_NAME: &str = "cf_breakpoint";
/// Name of meta column family
pub const CF_META_NAME: &str = "cf_meta";

pub type DbResult<T> = Result<T, DbError>;

pub struct RocksDBIO {
    pub db: DBWithThreadMode<MultiThreaded>,
}

impl RocksDBIO {
    pub fn open_or_create(path: &Path, start_diff: Option<Block>) -> DbResult<Self> {
        let mut cf_opts = Options::default();
        cf_opts.set_max_write_buffer_number(16);
        // ToDo: Add more column families for different data
        let cfdiff = ColumnFamilyDescriptor::new(CF_DIFF_NAME, cf_opts.clone());
        let cfmeta = ColumnFamilyDescriptor::new(CF_META_NAME, cf_opts.clone());
        let cfbr = ColumnFamilyDescriptor::new(CF_BREAKPOINT_NAME, cf_opts.clone());

        let mut db_opts = Options::default();
        db_opts.create_missing_column_families(true);
        db_opts.create_if_missing(true);
        let db = DBWithThreadMode::<MultiThreaded>::open_cf_descriptors(
            &db_opts,
            path,
            vec![cfdiff, cfmeta, cfbr],
        );

        let dbio = Self {
            // There is no point in handling this from runner code
            db: db.unwrap(),
        };

        let is_start_set = dbio.get_meta_is_first_diff_set()?;

        if is_start_set {
            Ok(dbio)
        } else if let Some(diff) = start_diff {
            let diff_id = diff.header.diff_id;
            dbio.put_meta_first_diff_in_db(diff)?;
            dbio.put_meta_is_first_diff_set()?;
            dbio.put_meta_last_diff_in_db(diff_id)?;

            Ok(dbio)
        } else {
            // Here we are trying to start a DB without a diff, one should not do it.
            unreachable!()
        }
    }

    pub fn destroy(path: &Path) -> DbResult<()> {
        let mut cf_opts = Options::default();
        cf_opts.set_max_write_buffer_number(16);
        // ToDo: Add more column families for different data
        let _cfb = ColumnFamilyDescriptor::new(CF_DIFF_NAME, cf_opts.clone());
        let _cfmeta = ColumnFamilyDescriptor::new(CF_META_NAME, cf_opts.clone());

        let mut db_opts = Options::default();
        db_opts.create_missing_column_families(true);
        db_opts.create_if_missing(true);
        DBWithThreadMode::<MultiThreaded>::destroy(&db_opts, path)
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))
    }

    pub fn meta_column(&self) -> Arc<BoundColumnFamily<'_>> {
        self.db.cf_handle(CF_META_NAME).unwrap()
    }

    pub fn diff_column(&self) -> Arc<BoundColumnFamily<'_>> {
        self.db.cf_handle(CF_DIFF_NAME).unwrap()
    }

    pub fn get_meta_first_diff_in_db(&self) -> DbResult<u64> {
        let cf_meta = self.meta_column();
        let res = self
            .db
            .get_cf(
                &cf_meta,
                borsh::to_vec(&DB_META_FIRST_DIFF_IN_DB_KEY).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize DB_META_FIRST_DIFF_IN_DB_KEY".to_string()),
                    )
                })?,
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        if let Some(data) = res {
            Ok(borsh::from_slice::<u64>(&data).map_err(|err| {
                DbError::borsh_cast_message(
                    err,
                    Some("Failed to deserialize first diff".to_string()),
                )
            })?)
        } else {
            Err(DbError::db_interaction_error(
                "First diff not found".to_string(),
            ))
        }
    }

    pub fn get_meta_last_diff_in_db(&self) -> DbResult<u64> {
        let cf_meta = self.meta_column();
        let res = self
            .db
            .get_cf(
                &cf_meta,
                borsh::to_vec(&DB_META_LAST_DIFF_IN_DB_KEY).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize DB_META_LAST_DIFF_IN_DB_KEY".to_string()),
                    )
                })?,
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        if let Some(data) = res {
            Ok(borsh::from_slice::<u64>(&data).map_err(|err| {
                DbError::borsh_cast_message(
                    err,
                    Some("Failed to deserialize last diff".to_string()),
                )
            })?)
        } else {
            Err(DbError::db_interaction_error(
                "Last diff not found".to_string(),
            ))
        }
    }

    pub fn get_meta_is_first_diff_set(&self) -> DbResult<bool> {
        let cf_meta = self.meta_column();
        let res = self
            .db
            .get_cf(
                &cf_meta,
                borsh::to_vec(&DB_META_FIRST_DIFF_SET_KEY).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize DB_META_FIRST_DIFF_SET_KEY".to_string()),
                    )
                })?,
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        Ok(res.is_some())
    }

    pub fn put_meta_first_diff_in_db(&self, diff: Block) -> DbResult<()> {
        let cf_meta = self.meta_column();
        self.db
            .put_cf(
                &cf_meta,
                borsh::to_vec(&DB_META_FIRST_DIFF_IN_DB_KEY).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize DB_META_FIRST_DIFF_IN_DB_KEY".to_string()),
                    )
                })?,
                borsh::to_vec(&diff.header.diff_id).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize first diff id".to_string()),
                    )
                })?,
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        self.put_diff(diff, true)?;
        Ok(())
    }

    pub fn put_meta_last_diff_in_db(&self, diff_id: u64) -> DbResult<()> {
        let cf_meta = self.meta_column();
        self.db
            .put_cf(
                &cf_meta,
                borsh::to_vec(&DB_META_LAST_DIFF_IN_DB_KEY).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize DB_META_LAST_DIFF_IN_DB_KEY".to_string()),
                    )
                })?,
                borsh::to_vec(&diff_id).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize last diff id".to_string()),
                    )
                })?,
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;
        Ok(())
    }

    pub fn put_meta_is_first_diff_set(&self) -> DbResult<()> {
        let cf_meta = self.meta_column();
        self.db
            .put_cf(
                &cf_meta,
                borsh::to_vec(&DB_META_FIRST_DIFF_SET_KEY).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize DB_META_FIRST_DIFF_SET_KEY".to_string()),
                    )
                })?,
                [1u8; 1],
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;
        Ok(())
    }

    pub fn put_diff(&self, diff: Block, first: bool) -> DbResult<()> {
        let cf_diff = self.diff_column();

        if !first {
            let last_curr_diff = self.get_meta_last_diff_in_db()?;

            if diff.header.diff_id > last_curr_diff {
                self.put_meta_last_diff_in_db(diff.header.diff_id)?;
            }
        }

        self.db
            .put_cf(
                &cf_diff,
                borsh::to_vec(&diff.header.diff_id).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize diff id".to_string()),
                    )
                })?,
                borsh::to_vec(&HashableBlockData::from(diff)).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize diff data".to_string()),
                    )
                })?,
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;
        Ok(())
    }

    pub fn get_diff(&self, diff_id: u64) -> DbResult<HashableBlockData> {
        let cf_diff = self.diff_column();
        let res = self
            .db
            .get_cf(
                &cf_diff,
                borsh::to_vec(&diff_id).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize diff id".to_string()),
                    )
                })?,
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        if let Some(data) = res {
            Ok(
                borsh::from_slice::<HashableBlockData>(&data).map_err(|serr| {
                    DbError::borsh_cast_message(
                        serr,
                        Some("Failed to deserialize diff data".to_string()),
                    )
                })?,
            )
        } else {
            Err(DbError::db_interaction_error(
                "Block on this id not found".to_string(),
            ))
        }
    }
}
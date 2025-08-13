use std::{path::Path, sync::Arc};

use common::block::{Block, HashableBlockData};
use error::DbError;
use rocksdb::{
    BoundColumnFamily, ColumnFamilyDescriptor, DBWithThreadMode, MultiThreaded, Options,
};
use sc_db_utils::{produce_blob_from_fit_vec, DataBlob, DataBlobChangeVariant};

pub mod error;
pub mod sc_db_utils;

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

///Size in bytes of a singular smart contract data blob, stored in db
pub const SC_DATA_BLOB_SIZE: usize = 256;

///Key base for storing metainformation about id of first block in db
pub const DB_META_FIRST_BLOCK_IN_DB_KEY: &str = "first_block_in_db";
///Key base for storing metainformation about id of last current block in db
pub const DB_META_LAST_BLOCK_IN_DB_KEY: &str = "last_block_in_db";
///Key base for storing metainformation which describe if first block has been set
pub const DB_META_FIRST_BLOCK_SET_KEY: &str = "first_block_set";
///Key to list of all known smart contract addresses
pub const DB_META_SC_LIST: &str = "sc_list";

///Key base for storing snapshot which describe block id
pub const DB_SNAPSHOT_BLOCK_ID_KEY: &str = "block_id";
///Key base for storing snapshot which describe commitment
pub const DB_SNAPSHOT_COMMITMENT_KEY: &str = "commitment";
///Key base for storing snapshot which describe transaction
pub const DB_SNAPSHOT_TRANSACTION_KEY: &str = "transaction";
///Key base for storing snapshot which describe nullifier
pub const DB_SNAPSHOT_NULLIFIER_KEY: &str = "nullifier";
///Key base for storing snapshot which describe account
pub const DB_SNAPSHOT_ACCOUNT_KEY: &str = "account";

///Name of block column family
pub const CF_BLOCK_NAME: &str = "cf_block";
///Name of meta column family
pub const CF_META_NAME: &str = "cf_meta";
///Name of smart contract column family
pub const CF_SC_NAME: &str = "cf_sc";
///Name of snapshot column family
pub const CF_SNAPSHOT_NAME: &str = "cf_snapshot";

///Suffix, used to mark field, which contain length of smart contract
pub const SC_LEN_SUFFIX: &str = "sc_len";

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
        let cfsc = ColumnFamilyDescriptor::new(CF_SC_NAME, cf_opts.clone());
        let cfsnapshot = ColumnFamilyDescriptor::new(CF_SNAPSHOT_NAME, cf_opts.clone());

        let mut db_opts = Options::default();
        db_opts.create_missing_column_families(true);
        db_opts.create_if_missing(true);
        let db = DBWithThreadMode::<MultiThreaded>::open_cf_descriptors(
            &db_opts,
            path,
            vec![cfb, cfmeta, cfsc, cfsnapshot],
        );

        let dbio = Self {
            //There is no point in handling this from runner code
            db: db.unwrap(),
        };

        let is_start_set = dbio.get_meta_is_first_block_set()?;

        if is_start_set {
            Ok(dbio)
        } else if let Some(block) = start_block {
            let block_id = block.block_id;
            dbio.put_meta_first_block_in_db(block)?;
            dbio.put_meta_is_first_block_set()?;

            dbio.put_meta_last_block_in_db(block_id)?;

            dbio.put_meta_sc_list(vec![])?;

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

    pub fn sc_column(&self) -> Arc<BoundColumnFamily<'_>> {
        self.db.cf_handle(CF_SC_NAME).unwrap()
    }

    pub fn snapshot_column(&self) -> Arc<BoundColumnFamily<'_>> {
        self.db.cf_handle(CF_SNAPSHOT_NAME).unwrap()
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

        self.put_block(block, true)?;
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

    ///Setting list of known smart contracts in a DB as a `sc_list`
    pub fn put_meta_sc_list(&self, sc_list: Vec<String>) -> DbResult<()> {
        let cf_meta = self.meta_column();
        self.db
            .put_cf(
                &cf_meta,
                DB_META_SC_LIST.as_bytes(),
                serde_json::to_vec(&sc_list).unwrap(),
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

    pub fn put_block(&self, block: Block, first: bool) -> DbResult<()> {
        let cf_block = self.block_column();

        if !first {
            let last_curr_block = self.get_meta_last_block_in_db()?;

            if block.block_id > last_curr_block {
                self.put_meta_last_block_in_db(block.block_id)?;
            }
        }

        self.db
            .put_cf(
                &cf_block,
                block.block_id.to_be_bytes(),
                HashableBlockData::from(block).to_bytes(),
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
            Ok(HashableBlockData::from_bytes(&data).into())
        } else {
            Err(DbError::db_interaction_error(
                "Block on this id not found".to_string(),
            ))
        }
    }

    ///Getting list of known smart contracts in a DB
    pub fn get_meta_sc_list(&self) -> DbResult<Vec<String>> {
        let cf_meta = self.meta_column();
        let sc_list = self
            .db
            .get_cf(&cf_meta, DB_META_SC_LIST)
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;
        if let Some(data) = sc_list {
            Ok(
                serde_json::from_slice::<Vec<String>>(&data).map_err(|serr| {
                    DbError::serde_cast_message(
                        serr,
                        Some("List of Sc Deserialization failed".to_string()),
                    )
                })?,
            )
        } else {
            Err(DbError::db_interaction_error(
                "Sc list not found".to_string(),
            ))
        }
    }

    ///Push additional contract into list of known contracts in a DB
    pub fn put_meta_sc(&self, sc_addr: String) -> DbResult<()> {
        let mut sc_list = self.get_meta_sc_list()?;
        if !sc_list.contains(&sc_addr) {
            sc_list.push(sc_addr);
        }
        self.put_meta_sc_list(sc_list)?;
        Ok(())
    }

    ///Put/Modify sc state in db
    pub fn put_sc_sc_state(
        &self,
        sc_addr: &str,
        length: usize,
        modifications: Vec<DataBlobChangeVariant>,
    ) -> DbResult<()> {
        self.put_meta_sc(sc_addr.to_string())?;

        let cf_sc = self.sc_column();

        let sc_addr_loc = format!("{sc_addr:?}{SC_LEN_SUFFIX}");
        let sc_len_addr = sc_addr_loc.as_bytes();

        self.db
            .put_cf(&cf_sc, sc_len_addr, length.to_be_bytes())
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        for data_change in modifications {
            match data_change {
                DataBlobChangeVariant::Created { id, blob } => {
                    let blob_addr = produce_address_for_data_blob_at_id(sc_addr, id);

                    self.db
                        .put_cf(&cf_sc, blob_addr, blob)
                        .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;
                }
                DataBlobChangeVariant::Modified {
                    id,
                    blob_old: _,
                    blob_new,
                } => {
                    let blob_addr = produce_address_for_data_blob_at_id(sc_addr, id);

                    self.db
                        .put_cf(&cf_sc, blob_addr, blob_new)
                        .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;
                }
                DataBlobChangeVariant::Deleted { id } => {
                    let blob_addr = produce_address_for_data_blob_at_id(sc_addr, id);

                    self.db
                        .delete_cf(&cf_sc, blob_addr)
                        .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;
                }
            }
        }

        Ok(())
    }

    ///Get sc state length in blobs from DB
    pub fn get_sc_sc_state_len(&self, sc_addr: &str) -> DbResult<usize> {
        let cf_sc = self.sc_column();
        let sc_addr_loc = format!("{sc_addr:?}{SC_LEN_SUFFIX}");

        let sc_len_addr = sc_addr_loc.as_bytes();

        let sc_len = self
            .db
            .get_cf(&cf_sc, sc_len_addr)
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        if let Some(sc_len) = sc_len {
            Ok(usize::from_be_bytes(sc_len.as_slice().try_into().unwrap()))
        } else {
            Err(DbError::db_interaction_error(format!(
                "Sc len for {sc_addr:?} not found"
            )))
        }
    }

    ///Get full sc state from DB
    pub fn get_sc_sc_state(&self, sc_addr: &str) -> DbResult<Vec<DataBlob>> {
        let cf_sc = self.sc_column();
        let sc_len = self.get_sc_sc_state_len(sc_addr)?;
        let mut data_blob_list = vec![];

        for id in 0..sc_len {
            let blob_addr = produce_address_for_data_blob_at_id(sc_addr, id);

            let blob = self
                .db
                .get_cf(&cf_sc, blob_addr)
                .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

            if let Some(blob_data) = blob {
                data_blob_list.push(produce_blob_from_fit_vec(blob_data));
            } else {
                return Err(DbError::db_interaction_error(format!(
                    "Blob for {sc_addr:?} at id {id} not found"
                )));
            }
        }

        Ok(data_blob_list)
    }

    pub fn get_snapshot_block_id(&self) -> DbResult<u64> {
        let cf_snapshot = self.snapshot_column();
        let res = self
            .db
            .get_cf(&cf_snapshot, DB_SNAPSHOT_BLOCK_ID_KEY)
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        if let Some(data) = res {
            Ok(u64::from_be_bytes(data.try_into().unwrap()))
        } else {
            Err(DbError::db_interaction_error(
                "Snapshot block ID not found".to_string(),
            ))
        }
    }

    pub fn get_snapshot_commitment(&self) -> DbResult<Vec<u8>> {
        let cf_snapshot = self.snapshot_column();
        let res = self
            .db
            .get_cf(&cf_snapshot, DB_SNAPSHOT_COMMITMENT_KEY)
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        if let Some(data) = res {
            Ok(data)
        } else {
            Err(DbError::db_interaction_error(
                "Snapshot commitment not found".to_string(),
            ))
        }
    }

    pub fn get_snapshot_transaction(&self) -> DbResult<Vec<u8>> {
        let cf_snapshot = self.snapshot_column();
        let res = self
            .db
            .get_cf(&cf_snapshot, DB_SNAPSHOT_TRANSACTION_KEY)
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        if let Some(data) = res {
            Ok(data)
        } else {
            Err(DbError::db_interaction_error(
                "Snapshot transaction not found".to_string(),
            ))
        }
    }

    pub fn get_snapshot_nullifier(&self) -> DbResult<Vec<u8>> {
        let cf_snapshot = self.snapshot_column();
        let res = self
            .db
            .get_cf(&cf_snapshot, DB_SNAPSHOT_NULLIFIER_KEY)
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        if let Some(data) = res {
            Ok(data)
        } else {
            Err(DbError::db_interaction_error(
                "Snapshot nullifier not found".to_string(),
            ))
        }
    }

    pub fn get_snapshot_account(&self) -> DbResult<Vec<u8>> {
        let cf_snapshot = self.snapshot_column();
        let res = self
            .db
            .get_cf(&cf_snapshot, DB_SNAPSHOT_ACCOUNT_KEY)
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        if let Some(data) = res {
            Ok(data)
        } else {
            Err(DbError::db_interaction_error(
                "Snapshot account not found".to_string(),
            ))
        }
    }

    pub fn put_snapshot_block_id_db(&self, block_id: u64) -> DbResult<()> {
        let cf_snapshot = self.snapshot_column();
        self.db
            .put_cf(
                &cf_snapshot,
                DB_SNAPSHOT_BLOCK_ID_KEY.as_bytes(),
                block_id.to_be_bytes(),
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;
        Ok(())
    }

    pub fn put_snapshot_commitement_db(&self, commitment: Vec<u8>) -> DbResult<()> {
        let cf_snapshot = self.snapshot_column();
        self.db
            .put_cf(
                &cf_snapshot,
                DB_SNAPSHOT_COMMITMENT_KEY.as_bytes(),
                commitment,
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;
        Ok(())
    }

    pub fn put_snapshot_transaction_db(&self, transaction: Vec<u8>) -> DbResult<()> {
        let cf_snapshot = self.snapshot_column();
        self.db
            .put_cf(
                &cf_snapshot,
                DB_SNAPSHOT_TRANSACTION_KEY.as_bytes(),
                transaction,
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;
        Ok(())
    }

    pub fn put_snapshot_nullifier_db(&self, nullifier: Vec<u8>) -> DbResult<()> {
        let cf_snapshot = self.snapshot_column();
        self.db
            .put_cf(
                &cf_snapshot,
                DB_SNAPSHOT_NULLIFIER_KEY.as_bytes(),
                nullifier,
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;
        Ok(())
    }

    pub fn put_snapshot_account_db(&self, account: Vec<u8>) -> DbResult<()> {
        let cf_snapshot = self.snapshot_column();
        self.db
            .put_cf(&cf_snapshot, DB_SNAPSHOT_ACCOUNT_KEY.as_bytes(), account)
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;
        Ok(())
    }
}

///Creates address for sc data blob at corresponding id
fn produce_address_for_data_blob_at_id(sc_addr: &str, id: usize) -> Vec<u8> {
    let mut prefix_bytes: Vec<u8> = sc_addr.as_bytes().to_vec();

    let id_bytes = id.to_be_bytes();

    for byte in id_bytes {
        prefix_bytes.push(byte);
    }

    prefix_bytes
}

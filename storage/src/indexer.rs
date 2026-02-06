use std::{collections::HashMap, ops::Div, path::Path, sync::Arc};

use common::{
    block::Block,
    transaction::{NSSATransaction, execute_check_transaction_on_state, transaction_pre_check},
};
use nssa::V02State;
use rocksdb::{
    BoundColumnFamily, ColumnFamilyDescriptor, DBWithThreadMode, MultiThreaded, Options, WriteBatch,
};

use crate::error::DbError;

/// Maximal size of stored blocks in base
///
/// Used to control db size
///
/// Currently effectively unbounded.
pub const BUFF_SIZE_ROCKSDB: usize = usize::MAX;

/// Size of stored blocks cache in memory
///
/// Keeping small to not run out of memory
pub const CACHE_SIZE: usize = 1000;

/// Key base for storing metainformation about id of first block in db
pub const DB_META_FIRST_BLOCK_IN_DB_KEY: &str = "first_block_in_db";
/// Key base for storing metainformation about id of last current block in db
pub const DB_META_LAST_BLOCK_IN_DB_KEY: &str = "last_block_in_db";
/// Key base for storing metainformation which describe if first block has been set
pub const DB_META_FIRST_BLOCK_SET_KEY: &str = "first_block_set";
/// Key base for storing metainformation about the last breakpoint
pub const DB_META_LAST_BREAKPOINT_ID: &str = "last_breakpoint_id";

/// Interval between state breakpoints
pub const BREAKPOINT_INTERVAL: u64 = 100;

/// Name of block column family
pub const CF_BLOCK_NAME: &str = "cf_block";
/// Name of meta column family
pub const CF_META_NAME: &str = "cf_meta";
/// Name of breakpoint column family
pub const CF_BREAKPOINT_NAME: &str = "cf_breakpoint";
/// Name of hash to id map column family
pub const CF_HASH_TO_ID: &str = "cf_hash_to_id";
/// Name of tx hash to id map column family
pub const CF_TX_TO_ID: &str = "cf_tx_to_id";
/// Name of account meta column family
pub const CF_ACC_META: &str = "cf_acc_meta";
/// Name of account id to tx hash map column family
pub const CF_ACC_TO_TX: &str = "cf_acc_to_tx";

pub type DbResult<T> = Result<T, DbError>;

fn closest_breakpoint_id(block_id: u64) -> u64 {
    block_id.div(BREAKPOINT_INTERVAL)
}

pub struct RocksDBIO {
    pub db: DBWithThreadMode<MultiThreaded>,
}

impl RocksDBIO {
    pub fn open_or_create(path: &Path, start_data: Option<(Block, V02State)>) -> DbResult<Self> {
        let mut cf_opts = Options::default();
        cf_opts.set_max_write_buffer_number(16);
        // ToDo: Add more column families for different data
        let cfb = ColumnFamilyDescriptor::new(CF_BLOCK_NAME, cf_opts.clone());
        let cfmeta = ColumnFamilyDescriptor::new(CF_META_NAME, cf_opts.clone());
        let cfbreakpoint = ColumnFamilyDescriptor::new(CF_BREAKPOINT_NAME, cf_opts.clone());
        let cfhti = ColumnFamilyDescriptor::new(CF_HASH_TO_ID, cf_opts.clone());
        let cftti = ColumnFamilyDescriptor::new(CF_TX_TO_ID, cf_opts.clone());
        let cfameta = ColumnFamilyDescriptor::new(CF_ACC_META, cf_opts.clone());
        let cfatt = ColumnFamilyDescriptor::new(CF_ACC_TO_TX, cf_opts.clone());

        let mut db_opts = Options::default();
        db_opts.create_missing_column_families(true);
        db_opts.create_if_missing(true);
        let db = DBWithThreadMode::<MultiThreaded>::open_cf_descriptors(
            &db_opts,
            path,
            vec![cfb, cfmeta, cfbreakpoint, cfhti, cftti, cfameta, cfatt],
        );

        let dbio = Self {
            // There is no point in handling this from runner code
            db: db.unwrap(),
        };

        let is_start_set = dbio.get_meta_is_first_block_set()?;

        if is_start_set {
            Ok(dbio)
        } else if let Some((block, initial_state)) = start_data {
            let block_id = block.header.block_id;
            dbio.put_meta_last_block_in_db(block_id)?;
            dbio.put_meta_first_block_in_db(block)?;
            dbio.put_meta_is_first_block_set()?;

            // First breakpoint setup
            dbio.put_breakpoint(0, initial_state)?;
            dbio.put_meta_last_breakpoint_id(0)?;

            Ok(dbio)
        } else {
            // Here we are trying to start a DB without a block, one should not do it.
            unreachable!()
        }
    }

    pub fn destroy(path: &Path) -> DbResult<()> {
        let mut cf_opts = Options::default();
        cf_opts.set_max_write_buffer_number(16);
        // ToDo: Add more column families for different data
        let _cfb = ColumnFamilyDescriptor::new(CF_BLOCK_NAME, cf_opts.clone());
        let _cfmeta = ColumnFamilyDescriptor::new(CF_META_NAME, cf_opts.clone());
        let _cfsnapshot = ColumnFamilyDescriptor::new(CF_BREAKPOINT_NAME, cf_opts.clone());
        let _cfhti = ColumnFamilyDescriptor::new(CF_HASH_TO_ID, cf_opts.clone());
        let _cftti = ColumnFamilyDescriptor::new(CF_TX_TO_ID, cf_opts.clone());
        let _cfameta = ColumnFamilyDescriptor::new(CF_ACC_META, cf_opts.clone());
        let _cfatt = ColumnFamilyDescriptor::new(CF_ACC_TO_TX, cf_opts.clone());

        let mut db_opts = Options::default();
        db_opts.create_missing_column_families(true);
        db_opts.create_if_missing(true);
        DBWithThreadMode::<MultiThreaded>::destroy(&db_opts, path)
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))
    }

    // Columns

    pub fn meta_column(&self) -> Arc<BoundColumnFamily<'_>> {
        self.db.cf_handle(CF_META_NAME).unwrap()
    }

    pub fn block_column(&self) -> Arc<BoundColumnFamily<'_>> {
        self.db.cf_handle(CF_BLOCK_NAME).unwrap()
    }

    pub fn breakpoint_column(&self) -> Arc<BoundColumnFamily<'_>> {
        self.db.cf_handle(CF_BREAKPOINT_NAME).unwrap()
    }

    pub fn hash_to_id_column(&self) -> Arc<BoundColumnFamily<'_>> {
        self.db.cf_handle(CF_HASH_TO_ID).unwrap()
    }

    pub fn tx_hash_to_id_column(&self) -> Arc<BoundColumnFamily<'_>> {
        self.db.cf_handle(CF_TX_TO_ID).unwrap()
    }

    pub fn account_id_to_tx_hash_column(&self) -> Arc<BoundColumnFamily<'_>> {
        self.db.cf_handle(CF_ACC_TO_TX).unwrap()
    }

    pub fn account_meta_column(&self) -> Arc<BoundColumnFamily<'_>> {
        self.db.cf_handle(CF_ACC_META).unwrap()
    }

    // Meta

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

    pub fn get_meta_last_breakpoint_id(&self) -> DbResult<u64> {
        let cf_meta = self.meta_column();
        let res = self
            .db
            .get_cf(
                &cf_meta,
                borsh::to_vec(&DB_META_LAST_BREAKPOINT_ID).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize DB_META_LAST_BREAKPOINT_ID".to_string()),
                    )
                })?,
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        if let Some(data) = res {
            Ok(borsh::from_slice::<u64>(&data).map_err(|err| {
                DbError::borsh_cast_message(
                    err,
                    Some("Failed to deserialize last breakpoint id".to_string()),
                )
            })?)
        } else {
            Err(DbError::db_interaction_error(
                "Last breakpoint id not found".to_string(),
            ))
        }
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

        self.put_block(block)?;
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

    pub fn put_meta_last_breakpoint_id(&self, br_id: u64) -> DbResult<()> {
        let cf_meta = self.meta_column();
        self.db
            .put_cf(
                &cf_meta,
                borsh::to_vec(&DB_META_LAST_BREAKPOINT_ID).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize DB_META_LAST_BREAKPOINT_ID".to_string()),
                    )
                })?,
                borsh::to_vec(&br_id).map_err(|err| {
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

    // Block

    pub fn put_block(&self, block: Block) -> DbResult<()> {
        let cf_block = self.block_column();
        let cf_hti = self.hash_to_id_column();
        let cf_tti = self.hash_to_id_column();

        // ToDo: rewrite this with write batching

        self.db
            .put_cf(
                &cf_block,
                borsh::to_vec(&block.header.block_id).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize block id".to_string()),
                    )
                })?,
                borsh::to_vec(&block).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize block data".to_string()),
                    )
                })?,
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        let last_curr_block = self.get_meta_last_block_in_db()?;

        if block.header.block_id > last_curr_block {
            self.put_meta_last_block_in_db(block.header.block_id)?;
        }

        self.db
            .put_cf(
                &cf_hti,
                borsh::to_vec(&block.header.hash).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize block hash".to_string()),
                    )
                })?,
                borsh::to_vec(&block.header.block_id).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize block id".to_string()),
                    )
                })?,
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        let mut acc_to_tx_map: HashMap<[u8; 32], Vec<[u8; 32]>> = HashMap::new();

        for tx in block.body.transactions {
            let tx_hash = tx.hash();

            self.db
                .put_cf(
                    &cf_tti,
                    borsh::to_vec(&tx_hash).map_err(|err| {
                        DbError::borsh_cast_message(
                            err,
                            Some("Failed to serialize tx hash".to_string()),
                        )
                    })?,
                    borsh::to_vec(&block.header.block_id).map_err(|err| {
                        DbError::borsh_cast_message(
                            err,
                            Some("Failed to serialize block id".to_string()),
                        )
                    })?,
                )
                .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

            let acc_ids = NSSATransaction::try_from(&tx)
                .map_err(|err| {
                    DbError::db_interaction_error(format!(
                        "failed to decode transaction in block {} with err {err:?}",
                        block.header.block_id
                    ))
                })?
                .affected_public_account_ids()
                .into_iter()
                .map(|account_id| account_id.into_value())
                .collect::<Vec<_>>();

            for acc_id in acc_ids {
                acc_to_tx_map
                    .entry(acc_id)
                    .and_modify(|tx_hashes| tx_hashes.push(tx_hash))
                    .or_insert(vec![tx_hash]);
            }
        }

        for (acc_id, tx_hashes) in acc_to_tx_map {
            self.put_account_transactions(acc_id, tx_hashes)?;
        }

        if block.header.block_id.is_multiple_of(BREAKPOINT_INTERVAL) {
            self.put_next_breakpoint()?;
        }

        Ok(())
    }

    pub fn get_block(&self, block_id: u64) -> DbResult<Block> {
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
            Ok(borsh::from_slice::<Block>(&data).map_err(|serr| {
                DbError::borsh_cast_message(
                    serr,
                    Some("Failed to deserialize block data".to_string()),
                )
            })?)
        } else {
            Err(DbError::db_interaction_error(
                "Block on this id not found".to_string(),
            ))
        }
    }

    pub fn get_block_batch(&self, offset: u64, limit: u64) -> DbResult<Vec<Block>> {
        let cf_block = self.block_column();
        let mut block_batch = vec![];

        // ToDo: Multi get this

        for block_id in offset..(offset + limit) {
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

            let block = if let Some(data) = res {
                Ok(borsh::from_slice::<Block>(&data).map_err(|serr| {
                    DbError::borsh_cast_message(
                        serr,
                        Some("Failed to deserialize block data".to_string()),
                    )
                })?)
            } else {
                // Block not found, assuming that previous one was the last
                break;
            }?;

            block_batch.push(block);
        }

        Ok(block_batch)
    }

    // State

    pub fn put_breakpoint(&self, br_id: u64, breakpoint: V02State) -> DbResult<()> {
        let cf_br = self.breakpoint_column();

        self.db
            .put_cf(
                &cf_br,
                borsh::to_vec(&br_id).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize breakpoint id".to_string()),
                    )
                })?,
                borsh::to_vec(&breakpoint).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize breakpoint data".to_string()),
                    )
                })?,
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))
    }

    pub fn get_breakpoint(&self, br_id: u64) -> DbResult<V02State> {
        let cf_br = self.breakpoint_column();
        let res = self
            .db
            .get_cf(
                &cf_br,
                borsh::to_vec(&br_id).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize breakpoint id".to_string()),
                    )
                })?,
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        if let Some(data) = res {
            Ok(borsh::from_slice::<V02State>(&data).map_err(|serr| {
                DbError::borsh_cast_message(
                    serr,
                    Some("Failed to deserialize breakpoint data".to_string()),
                )
            })?)
        } else {
            Err(DbError::db_interaction_error(
                "Breakpoint on this id not found".to_string(),
            ))
        }
    }

    pub fn calculate_state_for_id(&self, block_id: u64) -> DbResult<V02State> {
        let last_block = self.get_meta_last_block_in_db()?;

        if last_block <= block_id {
            let br_id = closest_breakpoint_id(block_id);
            let mut breakpoint = self.get_breakpoint(br_id)?;

            // ToDo: update it to handle any genesis id
            // right now works correctly only if genesis_id < BREAKPOINT_INTERVAL
            let start = if br_id != 0 {
                BREAKPOINT_INTERVAL * br_id
            } else {
                self.get_meta_first_block_in_db()?
            };

            for id in start..=block_id {
                let block = self.get_block(id)?;

                for encoded_transaction in block.body.transactions {
                    let transaction =
                        NSSATransaction::try_from(&encoded_transaction).map_err(|err| {
                            DbError::db_interaction_error(format!(
                                "failed to decode transaction in block {} with err {err:?}",
                                block.header.block_id
                            ))
                        })?;

                    execute_check_transaction_on_state(
                        &mut breakpoint,
                        transaction_pre_check(transaction).map_err(|err| {
                            DbError::db_interaction_error(format!(
                                "transaction pre check failed with err {err:?}"
                            ))
                        })?,
                    )
                    .map_err(|err| {
                        DbError::db_interaction_error(format!(
                            "transaction execution failed with err {err:?}"
                        ))
                    })?;
                }
            }

            Ok(breakpoint)
        } else {
            Err(DbError::db_interaction_error(
                "Block on this id not found".to_string(),
            ))
        }
    }

    pub fn final_state(&self) -> DbResult<V02State> {
        self.calculate_state_for_id(self.get_meta_last_block_in_db()?)
    }

    pub fn put_next_breakpoint(&self) -> DbResult<()> {
        let last_block = self.get_meta_last_block_in_db()?;
        let breakpoint_id = self.get_meta_last_breakpoint_id()?;
        let block_to_break_id = breakpoint_id * BREAKPOINT_INTERVAL;

        if last_block <= block_to_break_id {
            let next_breakpoint = self.calculate_state_for_id(block_to_break_id)?;

            self.put_breakpoint(breakpoint_id, next_breakpoint)?;
            self.put_meta_last_breakpoint_id(breakpoint_id)
        } else {
            Err(DbError::db_interaction_error(
                "Breakpoint not yet achieved".to_string(),
            ))
        }
    }

    // Mappings

    pub fn get_block_id_by_hash(&self, hash: [u8; 32]) -> DbResult<u64> {
        let cf_hti = self.hash_to_id_column();
        let res = self
            .db
            .get_cf(
                &cf_hti,
                borsh::to_vec(&hash).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize block hash".to_string()),
                    )
                })?,
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        if let Some(data) = res {
            Ok(borsh::from_slice::<u64>(&data).map_err(|serr| {
                DbError::borsh_cast_message(
                    serr,
                    Some("Failed to deserialize block id".to_string()),
                )
            })?)
        } else {
            Err(DbError::db_interaction_error(
                "Block on this hash not found".to_string(),
            ))
        }
    }

    pub fn get_block_id_by_tx_hash(&self, tx_hash: [u8; 32]) -> DbResult<u64> {
        let cf_tti = self.tx_hash_to_id_column();
        let res = self
            .db
            .get_cf(
                &cf_tti,
                borsh::to_vec(&tx_hash).map_err(|err| {
                    DbError::borsh_cast_message(
                        err,
                        Some("Failed to serialize block hash".to_string()),
                    )
                })?,
            )
            .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

        if let Some(data) = res {
            Ok(borsh::from_slice::<u64>(&data).map_err(|serr| {
                DbError::borsh_cast_message(
                    serr,
                    Some("Failed to deserialize block id".to_string()),
                )
            })?)
        } else {
            Err(DbError::db_interaction_error(
                "Block on this hash not found".to_string(),
            ))
        }
    }

    // Accounts meta

    fn update_acc_meta_batch(
        &self,
        acc_id: [u8; 32],
        num_tx: u64,
        write_batch: &mut WriteBatch,
    ) -> DbResult<()> {
        let cf_ameta = self.account_meta_column();

        write_batch.put_cf(
            &cf_ameta,
            borsh::to_vec(&acc_id).map_err(|err| {
                DbError::borsh_cast_message(err, Some("Failed to serialize account id".to_string()))
            })?,
            borsh::to_vec(&num_tx).map_err(|err| {
                DbError::borsh_cast_message(
                    err,
                    Some("Failed to serialize acc metadata".to_string()),
                )
            })?,
        );

        Ok(())
    }

    fn get_acc_meta_num_tx(&self, acc_id: [u8; 32]) -> DbResult<Option<u64>> {
        let cf_ameta = self.account_meta_column();
        let res = self.db.get_cf(&cf_ameta, acc_id).map_err(|rerr| {
            DbError::rocksdb_cast_message(rerr, Some("Failed to read from acc meta cf".to_string()))
        })?;

        res.map(|data| {
            borsh::from_slice::<u64>(&data).map_err(|serr| {
                DbError::borsh_cast_message(serr, Some("Failed to deserialize num tx".to_string()))
            })
        })
        .transpose()
    }

    // Account

    pub fn put_account_transactions(
        &self,
        acc_id: [u8; 32],
        tx_hashes: Vec<[u8; 32]>,
    ) -> DbResult<()> {
        let acc_num_tx = self.get_acc_meta_num_tx(acc_id)?.unwrap_or(0);
        let cf_att = self.account_id_to_tx_hash_column();
        let mut write_batch = WriteBatch::new();

        for (tx_id, tx_hash) in tx_hashes.iter().enumerate() {
            let put_id = acc_num_tx + tx_id as u64;

            let mut prefix = borsh::to_vec(&acc_id).map_err(|berr| {
                DbError::borsh_cast_message(
                    berr,
                    Some("Failed to serialize account id".to_string()),
                )
            })?;
            let suffix = borsh::to_vec(&put_id).map_err(|berr| {
                DbError::borsh_cast_message(berr, Some("Failed to serialize tx id".to_string()))
            })?;

            prefix.extend_from_slice(&suffix);

            write_batch.put_cf(
                &cf_att,
                prefix,
                borsh::to_vec(tx_hash).map_err(|berr| {
                    DbError::borsh_cast_message(
                        berr,
                        Some("Failed to serialize tx hash".to_string()),
                    )
                })?,
            );
        }

        self.update_acc_meta_batch(
            acc_id,
            acc_num_tx + (tx_hashes.len() as u64),
            &mut write_batch,
        )?;

        self.db.write(write_batch).map_err(|rerr| {
            DbError::rocksdb_cast_message(rerr, Some("Failed to write batch".to_string()))
        })
    }

    fn get_acc_transaction_hashes(
        &self,
        acc_id: [u8; 32],
        offset: u64,
        limit: u64,
    ) -> DbResult<Vec<[u8; 32]>> {
        let cf_att = self.account_id_to_tx_hash_column();
        let mut tx_batch = vec![];

        // ToDo: Multi get this

        for tx_id in offset..(offset + limit) {
            let mut prefix = borsh::to_vec(&acc_id).map_err(|berr| {
                DbError::borsh_cast_message(
                    berr,
                    Some("Failed to serialize account id".to_string()),
                )
            })?;
            let suffix = borsh::to_vec(&tx_id).map_err(|berr| {
                DbError::borsh_cast_message(berr, Some("Failed to serialize tx id".to_string()))
            })?;

            prefix.extend_from_slice(&suffix);

            let res = self
                .db
                .get_cf(&cf_att, prefix)
                .map_err(|rerr| DbError::rocksdb_cast_message(rerr, None))?;

            let tx_hash = if let Some(data) = res {
                Ok(borsh::from_slice::<[u8; 32]>(&data).map_err(|serr| {
                    DbError::borsh_cast_message(
                        serr,
                        Some("Failed to deserialize tx_hash".to_string()),
                    )
                })?)
            } else {
                // Tx hash not found, assuming that previous one was the last
                break;
            }?;

            tx_batch.push(tx_hash);
        }

        Ok(tx_batch)
    }

    pub fn get_acc_transactions(
        &self,
        acc_id: [u8; 32],
        offset: u64,
        limit: u64,
    ) -> DbResult<Vec<NSSATransaction>> {
        let mut tx_batch = vec![];

        for tx_hash in self.get_acc_transaction_hashes(acc_id, offset, limit)? {
            let block_id = self.get_block_id_by_hash(tx_hash)?;
            let block = self.get_block(block_id)?;

            let enc_tx = block
                .body
                .transactions
                .iter()
                .find(|tx| tx.hash() == tx_hash)
                .ok_or(DbError::db_interaction_error(format!(
                    "Missing transaction in block {} with hash {:#?}",
                    block.header.block_id, tx_hash
                )))?;

            let transaction = NSSATransaction::try_from(enc_tx).map_err(|err| {
                DbError::db_interaction_error(format!(
                    "failed to decode transaction in block {} with err {err:?}",
                    block.header.block_id
                ))
            })?;

            tx_batch.push(transaction);
        }

        Ok(tx_batch)
    }
}

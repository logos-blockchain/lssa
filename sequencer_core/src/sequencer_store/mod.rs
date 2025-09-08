use std::path::Path;

use block_store::SequecerBlockStore;
use common::block::HashableBlockData;
use nssa::{self, Address};
use rand::{RngCore, rngs::OsRng};

use crate::config::AccountInitialData;

pub mod block_store;

pub struct SequecerChainStore {
    pub state: nssa::V01State,
    pub block_store: SequecerBlockStore,
}

impl SequecerChainStore {
    pub fn new_with_genesis(
        home_dir: &Path,
        genesis_id: u64,
        is_genesis_random: bool,
        initial_accounts: &[AccountInitialData],
    ) -> Self {
        let init_accs: Vec<(Address, u128)> = initial_accounts
            .iter()
            .map(|acc_data| (acc_data.addr.parse().unwrap(), acc_data.balance))
            .collect();

        let state = nssa::V01State::new_with_genesis_accounts(&init_accs);

        let mut data = [0; 32];
        let mut prev_block_hash = [0; 32];

        if is_genesis_random {
            OsRng.fill_bytes(&mut data);
            OsRng.fill_bytes(&mut prev_block_hash);
        }

        let hashable_data = HashableBlockData {
            block_id: genesis_id,
            prev_block_id: genesis_id.saturating_sub(1),
            transactions: vec![],
            prev_block_hash,
        };

        let genesis_block = hashable_data.into();

        //Sequencer should panic if unable to open db,
        //as fixing this issue may require actions non-native to program scope
        let block_store = SequecerBlockStore::open_db_with_genesis(
            &home_dir.join("rocksdb"),
            Some(genesis_block),
        )
        .unwrap();

        Self { state, block_store }
    }
}

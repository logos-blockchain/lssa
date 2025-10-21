use std::path::Path;

use block_store::SequecerBlockStore;
use common::block::HashableBlockData;
use nssa::{self, Address};
use rand::{RngCore, rngs::OsRng};

use crate::config::AccountInitialData;

pub mod block_store;

pub struct SequecerChainStore {
    pub state: nssa::V02State,
    pub block_store: SequecerBlockStore,
}

impl SequecerChainStore {
    pub fn new_with_genesis(
        home_dir: &Path,
        genesis_id: u64,
        is_genesis_random: bool,
        initial_accounts: &[AccountInitialData],
        initial_commitments: &[nssa_core::Commitment],
        signing_key: nssa::PrivateKey,
    ) -> Self {
        let init_accs: Vec<(Address, u128)> = initial_accounts
            .iter()
            .map(|acc_data| (acc_data.addr.parse().unwrap(), acc_data.balance))
            .collect();

        #[cfg(not(feature = "testnet"))]
        let state = nssa::V02State::new_with_genesis_accounts(&init_accs, initial_commitments);

        #[cfg(feature = "testnet")]
        let state = {
            let mut this =
                nssa::V02State::new_with_genesis_accounts(&init_accs, initial_commitments);
            this.add_pinata_program("cafe".repeat(16).parse().unwrap());
            this
        };

        let mut data = [0; 32];
        let mut prev_block_hash = [0; 32];

        if is_genesis_random {
            OsRng.fill_bytes(&mut data);
            OsRng.fill_bytes(&mut prev_block_hash);
        }

        let curr_time = chrono::Utc::now().timestamp_millis() as u64;

        let hashable_data = HashableBlockData {
            block_id: genesis_id,
            transactions: vec![],
            prev_block_hash,
            timestamp: curr_time,
        };

        let genesis_block = hashable_data.into_block(&signing_key);

        //Sequencer should panic if unable to open db,
        //as fixing this issue may require actions non-native to program scope
        let block_store = SequecerBlockStore::open_db_with_genesis(
            &home_dir.join("rocksdb"),
            Some(genesis_block),
            signing_key,
        )
        .unwrap();

        Self { state, block_store }
    }
}

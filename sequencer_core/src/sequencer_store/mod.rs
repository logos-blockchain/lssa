use std::{collections::HashSet, path::Path};

use accounts_store::SequencerAccountsStore;
use block_store::SequecerBlockStore;
use common::{
    block::{Block, HashableBlockData},
    merkle_tree_public::merkle_tree::{PublicTransactionMerkleTree, UTXOCommitmentsMerkleTree},
    nullifier::UTXONullifier,
};
use rand::{rngs::OsRng, RngCore};

pub mod accounts_store;
pub mod block_store;

pub struct SequecerChainStore {
    pub acc_store: SequencerAccountsStore,
    pub block_store: SequecerBlockStore,
    pub nullifier_store: HashSet<UTXONullifier>,
    pub utxo_commitments_store: UTXOCommitmentsMerkleTree,
    pub pub_tx_store: PublicTransactionMerkleTree,
}

impl SequecerChainStore {
    pub fn new_with_genesis(home_dir: &Path, genesis_id: u64, is_genesis_random: bool) -> Self {
        let acc_store = SequencerAccountsStore::default();
        let nullifier_store = HashSet::new();
        let utxo_commitments_store = UTXOCommitmentsMerkleTree::new(vec![]);
        let pub_tx_store = PublicTransactionMerkleTree::new(vec![]);

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
            data: data.to_vec(),
            prev_block_hash,
        };

        let genesis_block = Block::produce_block_from_hashable_data(hashable_data);

        //Sequencer should panic if unable to open db,
        //as fixing this issue may require actions non-native to program scope
        let block_store = SequecerBlockStore::open_db_with_genesis(
            &home_dir.join("rocksdb"),
            Some(genesis_block),
        )
        .unwrap();

        Self {
            acc_store,
            block_store,
            nullifier_store,
            utxo_commitments_store,
            pub_tx_store,
        }
    }
}

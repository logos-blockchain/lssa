use std::path::Path;

use accounts::account_core::{Account, AccountAddress};
use accounts_store::NodeAccountsStore;
use block_store::NodeBlockStore;
use rand::{rngs::OsRng, RngCore};
use storage::{
    block::{Block, HashableBlockData},
    merkle_tree_public::merkle_tree::{PublicTransactionMerkleTree, UTXOCommitmentsMerkleTree},
    nullifier_sparse_merkle_tree::NullifierSparseMerkleTree,
};

pub mod accounts_store;
pub mod block_store;

pub struct NodeChainStore {
    pub acc_store: NodeAccountsStore,
    pub block_store: NodeBlockStore,
    pub nullifier_store: NullifierSparseMerkleTree,
    pub utxo_commitments_store: UTXOCommitmentsMerkleTree,
    pub pub_tx_store: PublicTransactionMerkleTree,
    ///For simplicity, we will allow only one account per node.
    /// ToDo: Change it in future
    node_main_account_info: Account,
}

impl NodeChainStore {
    pub fn new_with_genesis(home_dir: &Path, genesis_id: u64, is_genesis_random: bool) -> Self {
        let acc_store = NodeAccountsStore::default();
        let nullifier_store = NullifierSparseMerkleTree::default();
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
        let block_store =
            NodeBlockStore::open_db_with_genesis(&home_dir.join("rocksdb"), Some(genesis_block))
                .unwrap();

        Self {
            acc_store,
            block_store,
            nullifier_store,
            utxo_commitments_store,
            pub_tx_store,
            node_main_account_info: Account::new(),
        }
    }

    pub fn get_main_account_addr(&self) -> AccountAddress {
        self.node_main_account_info.address
    }
}

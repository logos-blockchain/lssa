use std::fmt::Display;

use anyhow::Result;
use common::{
    block::{Block, HashableBlockData},
    merkle_tree_public::TreeHashType,
    nullifier::UTXONullifier,
    transaction::{Transaction, TxKind},
    utxo_commitment::UTXOCommitment,
};
use config::SequencerConfig;
use mempool::MemPool;
use sequencer_store::{accounts_store::AccountPublicData, SequecerChainStore};
use serde::{Deserialize, Serialize};
use transaction_mempool::TransactionMempool;

pub mod config;
pub mod sequencer_store;
pub mod transaction_mempool;

pub struct SequencerCore {
    pub store: SequecerChainStore,
    pub mempool: MemPool<TransactionMempool>,
    pub sequencer_config: SequencerConfig,
    pub chain_height: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionMalformationErrorKind {
    PublicTransactionChangedPrivateData { tx: TreeHashType },
    PrivateTransactionChangedPublicData { tx: TreeHashType },
    TxHashAlreadyPresentInTree { tx: TreeHashType },
    NullifierAlreadyPresentInTree { tx: TreeHashType },
    UTXOCommitmentAlreadyPresentInTree { tx: TreeHashType },
    MempoolFullForRound { tx: TreeHashType },
    ChainStateFurtherThanTransactionState { tx: TreeHashType },
    FailedToInsert { tx: TreeHashType, details: String },
}

impl Display for TransactionMalformationErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:#?}")
    }
}

impl std::error::Error for TransactionMalformationErrorKind {}

impl SequencerCore {
    pub fn start_from_config(config: SequencerConfig) -> Self {
        Self {
            store: SequecerChainStore::new_with_genesis(
                &config.home,
                config.genesis_id,
                config.is_genesis_random,
            ),
            mempool: MemPool::<TransactionMempool>::default(),
            chain_height: config.genesis_id,
            sequencer_config: config,
        }
    }

    pub fn get_tree_roots(&self) -> [[u8; 32]; 3] {
        [
            self.store.nullifier_store.curr_root.unwrap_or([0; 32]),
            self.store
                .utxo_commitments_store
                .get_root()
                .unwrap_or([0; 32]),
            self.store.pub_tx_store.get_root().unwrap_or([0; 32]),
        ]
    }

    pub fn transaction_pre_check(
        &mut self,
        tx: &Transaction,
        tx_roots: [[u8; 32]; 3],
    ) -> Result<(), TransactionMalformationErrorKind> {
        let Transaction {
            hash,
            tx_kind,
            ref execution_input,
            ref execution_output,
            ref utxo_commitments_created_hashes,
            ref nullifier_created_hashes,
            ..
        } = tx;

        let mempool_size = self.mempool.len();

        if mempool_size >= self.sequencer_config.max_num_tx_in_block {
            return Err(TransactionMalformationErrorKind::MempoolFullForRound { tx: *hash });
        }

        let curr_sequencer_roots = self.get_tree_roots();

        if tx_roots != curr_sequencer_roots {
            return Err(
                TransactionMalformationErrorKind::ChainStateFurtherThanTransactionState {
                    tx: *hash,
                },
            );
        }

        //Sanity check
        match tx_kind {
            TxKind::Public => {
                if !utxo_commitments_created_hashes.is_empty()
                    || !nullifier_created_hashes.is_empty()
                {
                    //Public transactions can not make private operations.
                    return Err(
                        TransactionMalformationErrorKind::PublicTransactionChangedPrivateData {
                            tx: *hash,
                        },
                    );
                }
            }
            TxKind::Private => {
                if !execution_input.is_empty() || !execution_output.is_empty() {
                    //Not entirely necessary, but useful simplification for a future.
                    //This way only shielded and deshielded transactions can be used for interaction
                    //between public and private state.
                    return Err(
                        TransactionMalformationErrorKind::PrivateTransactionChangedPublicData {
                            tx: *hash,
                        },
                    );
                }
            }
            _ => {}
        };

        //Tree checks
        let tx_tree_check = self.store.pub_tx_store.get_tx(*hash).is_some();
        let nullifier_tree_check = nullifier_created_hashes
            .iter()
            .map(|nullifier_hash| {
                self.store
                    .nullifier_store
                    .search_item_inclusion(*nullifier_hash)
                    .unwrap_or(false)
            })
            .any(|check| check);
        let utxo_commitments_check = utxo_commitments_created_hashes
            .iter()
            .map(|utxo_commitment_hash| {
                self.store
                    .utxo_commitments_store
                    .get_tx(*utxo_commitment_hash)
                    .is_some()
            })
            .any(|check| check);

        if tx_tree_check {
            return Err(TransactionMalformationErrorKind::TxHashAlreadyPresentInTree { tx: *hash });
        }

        if nullifier_tree_check {
            return Err(
                TransactionMalformationErrorKind::NullifierAlreadyPresentInTree { tx: *hash },
            );
        }

        if utxo_commitments_check {
            return Err(
                TransactionMalformationErrorKind::UTXOCommitmentAlreadyPresentInTree { tx: *hash },
            );
        }

        Ok(())
    }

    pub fn push_tx_into_mempool_pre_check(
        &mut self,
        item: TransactionMempool,
        tx_roots: [[u8; 32]; 3],
    ) -> Result<(), TransactionMalformationErrorKind> {
        self.transaction_pre_check(&item.tx, tx_roots)?;

        self.mempool.push_item(item);

        Ok(())
    }

    fn execute_check_transaction_on_state(
        &mut self,
        tx: TransactionMempool,
    ) -> Result<(), TransactionMalformationErrorKind> {
        let Transaction {
            hash,
            ref utxo_commitments_created_hashes,
            ref nullifier_created_hashes,
            ..
        } = tx.tx;

        for utxo_comm in utxo_commitments_created_hashes {
            self.store
                .utxo_commitments_store
                .add_tx(UTXOCommitment { hash: *utxo_comm });
        }

        for nullifier in nullifier_created_hashes {
            self.store
                .nullifier_store
                .insert_item(UTXONullifier {
                    utxo_hash: *nullifier,
                })
                .map_err(|err| TransactionMalformationErrorKind::FailedToInsert {
                    tx: hash,
                    details: format!("{err:?}"),
                })?;
        }

        self.store.pub_tx_store.add_tx(tx.tx);

        Ok(())
    }

    pub fn register_account(&mut self, acc_data: AccountPublicData) {
        self.store
            .acc_store
            .accounts
            .insert(acc_data.address, acc_data);
    }

    ///Produces new block from transactions in mempool
    pub fn produce_new_block_with_mempool_transactions(&mut self) -> Result<u64> {
        let transactions = self
            .mempool
            .pop_size(self.sequencer_config.max_num_tx_in_block);

        for tx in transactions.clone() {
            self.execute_check_transaction_on_state(tx)?;
        }

        let prev_block_hash = self
            .store
            .block_store
            .get_block_at_id(self.chain_height)?
            .hash;

        let hashable_data = HashableBlockData {
            block_id: self.chain_height + 1,
            prev_block_id: self.chain_height,
            transactions: transactions.into_iter().map(|tx_mem| tx_mem.tx).collect(),
            data: vec![],
            prev_block_hash,
        };

        let block = Block::produce_block_from_hashable_data(hashable_data);

        self.store.block_store.put_block_at_id(block)?;

        self.chain_height += 1;

        Ok(self.chain_height - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use common::transaction::{Transaction, TxKind};
    use rand::Rng;
    use secp256k1_zkp::Tweak;
    use transaction_mempool::TransactionMempool;

    fn setup_sequencer_config() -> SequencerConfig {
        let mut rng = rand::thread_rng();
        let random_u8: u8 = rng.gen();

        let path_str = format!("/tmp/sequencer_{:?}", random_u8);

        SequencerConfig {
            home: PathBuf::from(path_str),
            override_rust_log: Some("info".to_string()),
            genesis_id: 1,
            is_genesis_random: false,
            max_num_tx_in_block: 10,
            block_create_timeout_millis: 1000,
            port: 8080,
        }
    }

    fn create_dummy_transaction(
        hash: TreeHashType,
        nullifier_created_hashes: Vec<[u8; 32]>,
        utxo_commitments_spent_hashes: Vec<[u8; 32]>,
        utxo_commitments_created_hashes: Vec<[u8; 32]>,
    ) -> Transaction {
        let mut rng = rand::thread_rng();

        Transaction {
            hash,
            tx_kind: TxKind::Private,
            execution_input: vec![],
            execution_output: vec![],
            utxo_commitments_spent_hashes,
            utxo_commitments_created_hashes,
            nullifier_created_hashes,
            execution_proof_private: "dummy_proof".to_string(),
            encoded_data: vec![],
            ephemeral_pub_key: vec![10, 11, 12],
            commitment: vec![],
            tweak: Tweak::new(&mut rng),
            secret_r: [0; 32],
        }
    }

    fn common_setup(mut sequencer: &mut SequencerCore) {
        let tx = create_dummy_transaction([12; 32], vec![[9; 32]], vec![[7; 32]], vec![[8; 32]]);
        let tx_mempool = TransactionMempool { tx };
        sequencer.mempool.push_item(tx_mempool);

        sequencer.produce_new_block_with_mempool_transactions();
    }

    #[test]
    fn test_start_from_config() {
        let config = setup_sequencer_config();
        let sequencer = SequencerCore::start_from_config(config.clone());

        assert_eq!(sequencer.chain_height, config.genesis_id);
        assert_eq!(sequencer.sequencer_config.max_num_tx_in_block, 10);
        assert_eq!(sequencer.sequencer_config.port, 8080);
    }

    #[test]
    fn test_get_tree_roots() {
        let config = setup_sequencer_config();
        let mut sequencer = SequencerCore::start_from_config(config);

        common_setup(&mut sequencer);

        let roots = sequencer.get_tree_roots();
        assert_eq!(roots.len(), 3); // Should return three roots
    }

    #[test]
    fn test_transaction_pre_check_pass() {
        let config = setup_sequencer_config();
        let mut sequencer = SequencerCore::start_from_config(config);

        common_setup(&mut sequencer);

        let tx = create_dummy_transaction([1; 32], vec![[91; 32]], vec![[71; 32]], vec![[81; 32]]);
        let tx_roots = sequencer.get_tree_roots();
        let result = sequencer.transaction_pre_check(&tx, tx_roots);

        assert!(result.is_ok());
    }

    #[test]
    fn test_transaction_pre_check_fail_mempool_full() {
        let config = SequencerConfig {
            max_num_tx_in_block: 1,
            ..setup_sequencer_config()
        };
        let mut sequencer = SequencerCore::start_from_config(config);

        common_setup(&mut sequencer);

        let tx = create_dummy_transaction([2; 32], vec![[92; 32]], vec![[72; 32]], vec![[82; 32]]);
        let tx_roots = sequencer.get_tree_roots();

        // Fill the mempool
        let dummy_tx = TransactionMempool { tx: tx.clone() };
        sequencer.mempool.push_item(dummy_tx);

        let result = sequencer.transaction_pre_check(&tx, tx_roots);

        assert!(matches!(
            result,
            Err(TransactionMalformationErrorKind::MempoolFullForRound { .. })
        ));
    }

    #[test]
    fn test_push_tx_into_mempool_pre_check() {
        let config = setup_sequencer_config();
        let mut sequencer = SequencerCore::start_from_config(config);

        common_setup(&mut sequencer);

        let tx = create_dummy_transaction([3; 32], vec![[93; 32]], vec![[73; 32]], vec![[83; 32]]);
        let tx_roots = sequencer.get_tree_roots();
        let tx_mempool = TransactionMempool { tx };

        let result = sequencer.push_tx_into_mempool_pre_check(tx_mempool.clone(), tx_roots);
        assert!(result.is_ok());
        assert_eq!(sequencer.mempool.len(), 1);
    }

    #[test]
    fn test_produce_new_block_with_mempool_transactions() {
        let config = setup_sequencer_config();
        let mut sequencer = SequencerCore::start_from_config(config);

        let tx = create_dummy_transaction([4; 32], vec![[94; 32]], vec![[7; 32]], vec![[8; 32]]);
        let tx_mempool = TransactionMempool { tx };
        sequencer.mempool.push_item(tx_mempool);

        let block_id = sequencer.produce_new_block_with_mempool_transactions();
        assert!(block_id.is_ok());
        assert_eq!(block_id.unwrap(), 1);
    }
}

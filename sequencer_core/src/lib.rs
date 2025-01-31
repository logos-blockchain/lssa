use std::fmt::Display;

use anyhow::Result;
use config::SequencerConfig;
use mempool::MemPool;
use sequecer_store::{accounts_store::AccountPublicData, SequecerChainStore};
use serde::{Deserialize, Serialize};
use storage::{
    block::{Block, HashableBlockData},
    merkle_tree_public::TreeHashType,
    nullifier::UTXONullifier,
    transaction::{Transaction, TxKind},
    utxo_commitment::UTXOCommitment,
};
use transaction_mempool::TransactionMempool;

pub mod config;
pub mod sequecer_store;
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
            self.store.utxo_commitments_store.get_root().unwrap_or([0; 32]),
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

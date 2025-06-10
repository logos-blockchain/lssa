use std::collections::{BTreeMap, HashMap, HashSet};

use accounts::account_core::{Account, AccountAddress};
use anyhow::Result;
use block_store::NodeBlockStore;
use common::{
    block::Block,
    merkle_tree_public::merkle_tree::{PublicTransactionMerkleTree, UTXOCommitmentsMerkleTree},
    nullifier::UTXONullifier,
    utxo_commitment::UTXOCommitment,
};
use k256::AffinePoint;
use log::{info, warn};
use public_context::PublicSCContext;
use utxo::utxo_core::UTXO;

use crate::{config::NodeConfig, ActionData};

pub mod accounts_store;
pub mod block_store;
pub mod public_context;

pub struct NodeChainStore {
    pub acc_map: HashMap<AccountAddress, Account>,
    pub block_store: NodeBlockStore,
    pub nullifier_store: HashSet<UTXONullifier>,
    pub utxo_commitments_store: UTXOCommitmentsMerkleTree,
    pub pub_tx_store: PublicTransactionMerkleTree,
    pub node_config: NodeConfig,
}

impl NodeChainStore {
    pub fn new(home_dir: &Path, genesis_block: Block) -> Result<(Self, u64)> {
        let mut acc_map = HashMap::new();
        let mut nullifier_store = HashSet::new();
        let mut utxo_commitments_store = UTXOCommitmentsMerkleTree::new(vec![]);
        let mut pub_tx_store = PublicTransactionMerkleTree::new(vec![]);
        let mut block_id = genesis_block.block_id;

        //Sequencer should panic if unable to open db,
        //as fixing this issue may require actions non-native to program scope
        let block_store =
            NodeBlockStore::open_db_with_genesis(&config.home.join("rocksdb"), Some(genesis_block))
                .unwrap();

        if let Ok(temp_block_id) = block_store.get_snapshot_block_id() {
            utxo_commitments_store = block_store.get_snapshot_commitment()?;
            nullifier_store = block_store.get_snapshot_nullifier()?;
            acc_map = block_store.get_snapshot_account()?;
            pub_tx_store = block_store.get_snapshot_transaction()?;
            block_id = temp_block_id;
        }

        Ok((
            Self {
                acc_map,
                block_store,
                nullifier_store,
                utxo_commitments_store,
                pub_tx_store,
            },
            block_id,
        ))
    }

    pub fn dissect_insert_block(&mut self, block: Block) -> Result<()> {
        let block_id = block.block_id;

        for tx in &block.transactions {
            if !tx.execution_input.is_empty() {
                let public_action = serde_json::from_slice::<ActionData>(&tx.execution_input);

                if let Ok(public_action) = public_action {
                    match public_action {
                        ActionData::MintMoneyPublicTx(action) => {
                            let acc_mut = self.acc_map.get_mut(&action.acc);

                            if let Some(acc_mut) = acc_mut {
                                acc_mut.balance += action.amount as u64;
                            }
                        }
                        ActionData::SendMoneyDeshieldedTx(action) => {
                            for (balance, acc_addr) in action.receiver_data {
                                let acc_mut = self.acc_map.get_mut(&acc_addr);

                                if let Some(acc_mut) = acc_mut {
                                    acc_mut.balance += balance as u64;
                                }
                            }
                        }
                        ActionData::SendMoneyShieldedTx(action) => {
                            let acc_mut = self.acc_map.get_mut(&action.acc_sender);

                            if let Some(acc_mut) = acc_mut {
                                acc_mut.balance =
                                    acc_mut.balance.saturating_sub(action.amount as u64);
                            }
                        }
                        _ => {}
                    }
                }
            }

            self.utxo_commitments_store.add_tx_multiple(
                tx.utxo_commitments_created_hashes
                    .clone()
                    .into_iter()
                    .map(|hash| UTXOCommitment { hash })
                    .collect(),
            );

            for nullifier in tx.nullifier_created_hashes.iter() {
                self.nullifier_store.insert(UTXONullifier {
                    utxo_hash: *nullifier,
                });
            }

            if !tx.encoded_data.is_empty() {
                let ephemeral_public_key_sender =
                    serde_json::from_slice::<AffinePoint>(&tx.ephemeral_pub_key)?;

                for (ciphertext, nonce, tag) in tx.encoded_data.clone() {
                    let slice = nonce.as_slice();
                    let nonce =
                        accounts::key_management::constants_types::Nonce::clone_from_slice(slice);
                    for (acc_id, acc) in self.acc_map.iter_mut() {
                        if acc_id[0] == tag {
                            let decoded_data_curr_acc = acc.decrypt_data(
                                ephemeral_public_key_sender,
                                ciphertext.clone(),
                                nonce,
                            );
                            if let Ok(decoded_data_curr_acc) = decoded_data_curr_acc {
                                let decoded_utxo_try =
                                    serde_json::from_slice::<UTXO>(&decoded_data_curr_acc);
                                if let Ok(utxo) = decoded_utxo_try {
                                    if &utxo.owner == acc_id {
                                        acc.utxos.insert(utxo.hash, utxo);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            self.pub_tx_store.add_tx(tx.clone());
        }

        self.block_store.put_block_at_id(block)?;

        //Snapshot
        if block_id % self.node_config.shapshot_frequency_in_blocks == 0 {
            //Serializing all important data structures

            //If we fail serialization, it is not the reason to stop running
            //Logging on warn level in this cases

            if let Ok(accounts_ser) = serde_json::to_vec(&self.acc_map).inspect_err(|err| {
                warn!("Failed to serialize accounts data {err:#?}");
            }) {
                if let Ok(comm_ser) =
                    serde_json::to_vec(&self.utxo_commitments_store).inspect_err(|err| {
                        warn!("Failed to serialize commitments {err:#?}");
                    })
                {
                    if let Ok(txs_ser) = serde_json::to_vec(&self.pub_tx_store).inspect_err(|err| {
                        warn!("Failed to serialize transactions {err:#?}");
                    }) {
                        if let Ok(nullifiers_ser) = serde_json::to_vec(&self.nullifier_store)
                            .inspect_err(|err| {
                                warn!("Failed to serialize nullifiers {err:#?}");
                            })
                        {
                            let snapshot_trace = self.block_store.put_snapshot_at_block_id(
                                block_id,
                                accounts_ser,
                                comm_ser,
                                txs_ser,
                                nullifiers_ser,
                            );

                            info!(
                                "Snapshot executed at {:?} with results {snapshot_trace:#?}",
                                block_id
                            );
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub fn produce_context(&self, caller: AccountAddress) -> PublicSCContext {
        let mut account_masks = BTreeMap::new();

        for (acc_addr, acc) in &self.acc_map {
            account_masks.insert(*acc_addr, acc.make_account_public_mask());
        }

        PublicSCContext {
            caller_address: caller,
            caller_balance: self.acc_map.get(&caller).unwrap().balance,
            account_masks,
            comitment_store_root: self.utxo_commitments_store.get_root().unwrap_or([0; 32]),
            pub_tx_store_root: self.pub_tx_store.get_root().unwrap_or([0; 32]),
        }
    }
}

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
use serde::{Deserialize, Serialize};
use utxo::utxo_core::UTXO;

use crate::{config::NodeConfig, ActionData};

pub mod accounts_store;
pub mod block_store;
pub mod public_context;

#[derive(Deserialize, Serialize)]
pub struct AccMap {
    pub acc_map: HashMap<String, Account>,
}

impl From<HashMap<[u8; 32], Account>> for AccMap {
    fn from(value: HashMap<[u8; 32], Account>) -> Self {
        AccMap {
            acc_map: value
                .into_iter()
                .map(|(key, val)| (hex::encode(key), val))
                .collect(),
        }
    }
}

impl From<AccMap> for HashMap<[u8; 32], Account> {
    fn from(value: AccMap) -> Self {
        value
            .acc_map
            .into_iter()
            .map(|(key, val)| (hex::decode(key).unwrap().try_into().unwrap(), val))
            .collect()
    }
}

pub struct NodeChainStore {
    pub acc_map: HashMap<AccountAddress, Account>,
    pub block_store: NodeBlockStore,
    pub nullifier_store: HashSet<UTXONullifier>,
    pub utxo_commitments_store: UTXOCommitmentsMerkleTree,
    pub pub_tx_store: PublicTransactionMerkleTree,
    pub node_config: NodeConfig,
}

impl NodeChainStore {
    pub fn new(config: NodeConfig, genesis_block: Block) -> Result<(Self, u64)> {
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
                acc_map: From::from(acc_map),
                block_store,
                nullifier_store,
                utxo_commitments_store,
                pub_tx_store,
                node_config: config,
            },
            block_id,
        ))
    }

    pub fn new_after_restart(config: NodeConfig, genesis_block: Block) -> Result<(Self, u64)> {
        let mut acc_map = HashMap::new();
        let mut nullifier_store = HashSet::new();
        let mut utxo_commitments_store = UTXOCommitmentsMerkleTree::new(vec![]);
        let mut pub_tx_store = PublicTransactionMerkleTree::new(vec![]);
        let mut block_id = genesis_block.block_id;

        //Sequencer should panic if unable to open db,
        //as fixing this issue may require actions non-native to program scope
        let block_store = NodeBlockStore::open_db_reload(&config.home.join("rocksdb")).unwrap();

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
                node_config: config,
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
                        if hex::decode(acc_id).unwrap()[0] == tag {
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
            let acc_map: AccMap = self.acc_map.clone().into();

            if let Ok(accounts_ser) = serde_json::to_vec(&acc_map).inspect_err(|err| {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GasConfig;
    use accounts::account_core::Account;
    use common::block::{Block, Data};
    use common::merkle_tree_public::TreeHashType;
    use common::transaction::{Transaction, TxKind};
    use secp256k1_zkp::Tweak;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn create_genesis_block() -> Block {
        Block {
            block_id: 0,
            prev_block_id: 0,
            prev_block_hash: [0; 32],
            hash: [1; 32],
            transactions: vec![],
            data: Data::default(),
        }
    }

    fn create_dummy_transaction(
        hash: TreeHashType,
        // execution_input: Vec<u8>,
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
            sc_addr: "sc_addr".to_string(),
            state_changes: (serde_json::Value::Null, 0),
        }
    }

    fn create_sample_block(block_id: u64, prev_block_id: u64) -> Block {
        Block {
            block_id: block_id,
            prev_block_id: prev_block_id,
            prev_block_hash: [0; 32],
            hash: [1; 32],
            transactions: vec![],
            data: Data::default(),
        }
    }

    fn create_sample_node_config(home: PathBuf) -> NodeConfig {
        NodeConfig {
            home,
            override_rust_log: None,
            sequencer_addr: "http://127.0.0.1".to_string(),
            seq_poll_timeout_secs: 1,
            port: 8000,
            gas_config: create_sample_gas_config(),
            shapshot_frequency_in_blocks: 1,
        }
    }

    fn create_sample_gas_config() -> GasConfig {
        GasConfig {
            gas_fee_per_byte_deploy: 0,
            gas_fee_per_input_buffer_runtime: 0,
            gas_fee_per_byte_runtime: 0,
            gas_cost_runtime: 0,
            gas_cost_deploy: 0,
            gas_limit_deploy: 0,
            gas_limit_runtime: 0,
        }
    }

    fn generate_dummy_utxo(address: TreeHashType, amount: u128) -> UTXO {
        UTXO::new(address, vec![], amount, false)
    }

    #[test]
    fn test_new_initializes_correctly() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path();

        let config = create_sample_node_config(path.to_path_buf());

        let genesis_block = create_genesis_block();

        let (store, block_id) = NodeChainStore::new(config.clone(), genesis_block.clone()).unwrap();

        assert_eq!(block_id, 0);
        assert!(store.acc_map.is_empty());
        assert!(store.nullifier_store.is_empty());
        assert_eq!(
            store.utxo_commitments_store.get_root().unwrap_or([0; 32]),
            [0; 32]
        );
    }

    #[test]
    fn test_new_recovers_from_snapshot() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().to_path_buf();

        let config = create_sample_node_config(path);

        let nullifier_secret_const =
            "261d61d294ac4bdc24f91b6f490efa263757a4a95f65871cd4f16b2ea23c3b5d";
        std::env::set_var("NULLIFIER_SECRET_CONST", nullifier_secret_const);

        let viewing_secret_const =
            "6117af750b30d7a296672ec3b3b25d3489beca3cfe5770fa39f275cec395d5ce";
        std::env::set_var("VIEWING_SECRET_CONST", viewing_secret_const);

        let genesis_block = create_genesis_block();

        // Initialize once to create DB and store fake snapshot
        {
            let (mut store, _) =
                NodeChainStore::new(config.clone(), genesis_block.clone()).unwrap();

            // Insert state
            let mut account = Account::new();
            account
                .add_new_utxo_outputs(vec![generate_dummy_utxo(account.address, 100)])
                .unwrap();
            store.acc_map.insert(account.address, account);
            store.nullifier_store.insert(UTXONullifier {
                utxo_hash: [2u8; 32],
            });
            store
                .utxo_commitments_store
                .add_tx_multiple(vec![UTXOCommitment { hash: [3u8; 32] }]);
            store.pub_tx_store.add_tx(create_dummy_transaction(
                [12; 32],
                vec![[9; 32]],
                vec![[7; 32]],
                vec![[8; 32]],
            ));

            // Put block snapshot to trigger snapshot recovery on next load
            let dummy_block = create_sample_block(1, 0);

            store.dissect_insert_block(dummy_block).unwrap();
        }

        // Now reload and verify snapshot is used
        let (recovered_store, block_id) =
            NodeChainStore::new_after_restart(config.clone(), genesis_block).unwrap();

        assert_eq!(block_id, 1);
        assert_eq!(recovered_store.acc_map.len(), 1);
        assert_eq!(
            recovered_store.utxo_commitments_store.get_root().is_some(),
            true
        );
    }
}

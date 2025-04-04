use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
};

use accounts::account_core::{Account, AccountAddress};
use anyhow::Result;
use block_store::NodeBlockStore;
use elliptic_curve::group::GroupEncoding;
use k256::AffinePoint;
use public_context::PublicSCContext;
use storage::{
    block::Block,
    merkle_tree_public::merkle_tree::{PublicTransactionMerkleTree, UTXOCommitmentsMerkleTree},
    nullifier::UTXONullifier,
    nullifier_sparse_merkle_tree::NullifierSparseMerkleTree,
    utxo_commitment::UTXOCommitment,
};
use utxo::utxo_core::UTXO;

use crate::ActionData;

pub mod accounts_store;
pub mod block_store;
pub mod public_context;

pub struct NodeChainStore {
    pub acc_map: HashMap<AccountAddress, Account>,
    pub block_store: NodeBlockStore,
    pub nullifier_store: NullifierSparseMerkleTree,
    pub utxo_commitments_store: UTXOCommitmentsMerkleTree,
    pub pub_tx_store: PublicTransactionMerkleTree,
}

impl NodeChainStore {
    pub fn new_with_genesis(home_dir: &Path, genesis_block: Block) -> Self {
        let acc_map = HashMap::new();
        let nullifier_store = NullifierSparseMerkleTree::default();
        let utxo_commitments_store = UTXOCommitmentsMerkleTree::new(vec![]);
        let pub_tx_store = PublicTransactionMerkleTree::new(vec![]);

        //Sequencer should panic if unable to open db,
        //as fixing this issue may require actions non-native to program scope
        let block_store =
            NodeBlockStore::open_db_with_genesis(&home_dir.join("rocksdb"), Some(genesis_block))
                .unwrap();

        Self {
            acc_map,
            block_store,
            nullifier_store,
            utxo_commitments_store,
            pub_tx_store,
        }
    }

    pub fn dissect_insert_block(&mut self, block: Block) -> Result<()> {
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

            self.nullifier_store.insert_items(
                tx.nullifier_created_hashes
                    .clone()
                    .into_iter()
                    .map(|hash| UTXONullifier { utxo_hash: hash })
                    .collect(),
            )?;

            let slice_try: Result<[u8; 33], _> = tx.ephemeral_pub_key.clone().try_into();
            let eph_key_compressed =
                slice_try.and_then(|inner| Ok(<AffinePoint as GroupEncoding>::Repr::from(inner)));

            if let Ok(eph_key_compressed) = eph_key_compressed {
                let ephemeral_public_key_sender = AffinePoint::from_bytes(&eph_key_compressed);

                if ephemeral_public_key_sender.is_some().into() {
                    let ephemeral_public_key_sender = ephemeral_public_key_sender.unwrap();

                    for (ciphertext, nonce, tag) in tx.encoded_data.clone() {
                        let slice = nonce.as_slice();
                        let nonce =
                            accounts::key_management::constants_types::Nonce::clone_from_slice(
                                slice,
                            );

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
                                            acc.utxo_tree.insert_item(utxo)?;
                                        }
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
            nullifier_store_root: self.nullifier_store.curr_root.unwrap_or([0; 32]),
            comitment_store_root: self.utxo_commitments_store.get_root().unwrap_or([0; 32]),
            pub_tx_store_root: self.pub_tx_store.get_root().unwrap_or([0; 32]),
        }
    }
}

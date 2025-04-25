use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use common::ExecutionFailureKind;

use accounts::account_core::{Account, AccountAddress};
use anyhow::Result;
use chain_storage::NodeChainStore;
use common::transaction::{Transaction, TransactionPayload, TxKind};
use config::NodeConfig;
use executions::private_exec::{generate_commitments, generate_nullifiers};
use log::info;
use sc_core::proofs_circuits::pedersen_commitment_vec;
use sequencer_client::{json::SendTxResponse, SequencerClient};
use serde::{Deserialize, Serialize};
use storage::sc_db_utils::DataBlobChangeVariant;
use tokio::{sync::RwLock, task::JoinHandle};
use utxo::utxo_core::UTXO;
use zkvm::{
    gas_calculator::GasCalculator, prove_mint_utxo, prove_mint_utxo_multiple_assets,
    prove_send_utxo, prove_send_utxo_deshielded, prove_send_utxo_multiple_assets_one_receiver,
    prove_send_utxo_shielded,
};

pub const BLOCK_GEN_DELAY_SECS: u64 = 20;

pub mod chain_storage;
pub mod config;
pub mod executions;
///Module, which includes pre start setup helperfunctions  
pub mod pre_start;
pub mod sequencer_client;

fn vec_u8_to_vec_u64(bytes: Vec<u8>) -> Vec<u64> {
    // Pad with zeros to make sure it's a multiple of 8
    let mut padded = bytes.clone();
    while padded.len() % 8 != 0 {
        padded.push(0);
    }

    padded
        .chunks(8)
        .map(|chunk| {
            let mut array = [0u8; 8];
            array.copy_from_slice(chunk);
            u64::from_le_bytes(array)
        })
        .collect()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MintMoneyPublicTx {
    pub acc: AccountAddress,
    pub amount: u128,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SendMoneyShieldedTx {
    pub acc_sender: AccountAddress,
    pub amount: u128,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SendMoneyDeshieldedTx {
    pub receiver_data: Vec<(u128, AccountAddress)>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UTXOPublication {
    pub utxos: Vec<UTXO>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ActionData {
    MintMoneyPublicTx(MintMoneyPublicTx),
    SendMoneyShieldedTx(SendMoneyShieldedTx),
    SendMoneyDeshieldedTx(SendMoneyDeshieldedTx),
    UTXOPublication(UTXOPublication),
}

pub struct NodeCore {
    pub storage: Arc<RwLock<NodeChainStore>>,
    pub curr_height: Arc<AtomicU64>,
    pub node_config: NodeConfig,
    pub db_updater_handle: JoinHandle<Result<()>>,
    pub sequencer_client: Arc<SequencerClient>,
    pub gas_calculator: GasCalculator,
}

impl NodeCore {
    pub async fn start_from_config_update_chain(config: NodeConfig) -> Result<Self> {
        let client = Arc::new(SequencerClient::new(config.clone())?);

        let genesis_id = client.get_genesis_id().await?;
        info!("Gesesis id is {genesis_id:?}");

        let genesis_block = client.get_block(genesis_id.genesis_id).await?.block;

        let mut storage = NodeChainStore::new_with_genesis(&config.home, genesis_block);

        pre_start::setup_empty_sc_states(&storage).await?;

        let mut chain_height = genesis_id.genesis_id;

        //Chain update loop
        loop {
            let next_block = chain_height + 1;

            if let Ok(block) = client.get_block(next_block).await {
                storage.dissect_insert_block(block.block)?;
                info!("Preprocessed block with id {next_block:?}");
            } else {
                break;
            }

            chain_height += 1;
        }

        let wrapped_storage = Arc::new(RwLock::new(storage));
        let chain_height_wrapped = Arc::new(AtomicU64::new(chain_height));

        let wrapped_storage_thread = wrapped_storage.clone();
        let wrapped_chain_height_thread = chain_height_wrapped.clone();
        let client_thread = client.clone();

        let updater_handle = tokio::spawn(async move {
            loop {
                let next_block = wrapped_chain_height_thread.load(Ordering::Relaxed) + 1;

                if let Ok(block) = client_thread.get_block(next_block).await {
                    {
                        let mut storage_guard = wrapped_storage_thread.write().await;

                        let block_insertion_result =
                            storage_guard.dissect_insert_block(block.block);

                        if block_insertion_result.is_err() {
                            info!("Block insertion failed due to {block_insertion_result:?}");

                            block_insertion_result?;
                        }
                        info!("Processed block with id {next_block:?}");
                    }

                    wrapped_chain_height_thread.store(next_block, Ordering::Relaxed);
                } else {
                    tokio::time::sleep(std::time::Duration::from_secs(
                        config.seq_poll_timeout_secs,
                    ))
                    .await;
                }
            }
        });

        Ok(Self {
            storage: wrapped_storage,
            curr_height: chain_height_wrapped,
            node_config: config.clone(),
            db_updater_handle: updater_handle,
            sequencer_client: client.clone(),
            gas_calculator: GasCalculator::from(config.gas_config),
        })
    }

    pub async fn get_roots(&self) -> [[u8; 32]; 3] {
        let storage = self.storage.read().await;
        [
            storage.nullifier_store.curr_root.unwrap_or([0; 32]),
            storage.utxo_commitments_store.get_root().unwrap_or([0; 32]),
            storage.pub_tx_store.get_root().unwrap_or([0; 32]),
        ]
    }

    pub async fn create_new_account(&mut self) -> AccountAddress {
        let account = Account::new();
        account.log();

        let addr = account.address;

        {
            let mut write_guard = self.storage.write().await;

            write_guard.acc_map.insert(account.address, account);
        }

        addr
    }

    pub async fn mint_utxo_private(
        &self,
        acc: AccountAddress,
        amount: u128,
    ) -> Result<(Transaction, [u8; 32]), ExecutionFailureKind> {
        let (utxo, receipt) = prove_mint_utxo(amount, acc)?;
        let result_hash = utxo.hash;

        let acc_map_read_guard = self.storage.read().await;

        let account = acc_map_read_guard.acc_map.get(&acc).unwrap();

        let ephm_key_holder = &account.produce_ephemeral_key_holder();
        ephm_key_holder.log();

        let eph_pub_key =
            serde_json::to_vec(&ephm_key_holder.generate_ephemeral_public_key()).unwrap();

        let encoded_data = Account::encrypt_data(
            &ephm_key_holder,
            account.key_holder.viewing_public_key,
            &serde_json::to_vec(&utxo).unwrap(),
        );

        let tag = account.make_tag();

        let comm = generate_commitments(&vec![utxo]);

        let mint_utxo_addr_bytes: Vec<u8> = zkvm::test_methods::MINT_UTXO_ID
            .iter()
            .map(|num| num.to_le_bytes())
            .flatten()
            .collect();
        let sc_addr = hex::encode(mint_utxo_addr_bytes);

        //Sc does not change its state
        let state_changes: Vec<DataBlobChangeVariant> = vec![];
        let new_len = 0;
        let state_changes = (serde_json::to_value(state_changes).unwrap(), new_len);

        let sc_state = acc_map_read_guard
            .block_store
            .get_sc_sc_state(&sc_addr)
            .map_err(ExecutionFailureKind::db_error)?;

        let mut vec_values_u64: Vec<Vec<u64>> = sc_state
            .into_iter()
            .map(|slice| vec_u8_to_vec_u64(slice.to_vec()))
            .collect();

        let context = acc_map_read_guard.produce_context(account.address);

        //Will not panic, as PublicScContext is serializable
        let context_public_info: Vec<u64> = context.produce_u64_list_from_context().unwrap();
        vec_values_u64.push(context_public_info);

        let vec_public_info: Vec<u64> = vec_values_u64.into_iter().flatten().collect();

        let (tweak, secret_r, commitment) = pedersen_commitment_vec(vec_public_info);

        Ok((
            TransactionPayload {
                tx_kind: TxKind::Private,
                execution_input: vec![],
                execution_output: vec![],
                utxo_commitments_spent_hashes: vec![],
                utxo_commitments_created_hashes: comm
                    .into_iter()
                    .map(|hash_data| hash_data.try_into().unwrap())
                    .collect(),
                nullifier_created_hashes: vec![],
                execution_proof_private: sc_core::transaction_payloads_tools::encode_receipt(
                    receipt,
                )
                .unwrap(),
                encoded_data: vec![(encoded_data.0, encoded_data.1.to_vec(), tag)],
                ephemeral_pub_key: eph_pub_key.to_vec(),
                commitment,
                tweak,
                secret_r,
                sc_addr,
                state_changes,
            }
            .into(),
            result_hash,
        ))
    }

    pub async fn mint_utxo_multiple_assets_private(
        &self,
        acc: AccountAddress,
        amount: u128,
        number_of_assets: usize,
    ) -> Result<(Transaction, Vec<[u8; 32]>), ExecutionFailureKind> {
        let (utxos, receipt) = prove_mint_utxo_multiple_assets(amount, number_of_assets, acc)?;
        let result_hashes = utxos.iter().map(|utxo| utxo.hash).collect();

        let acc_map_read_guard = self.storage.read().await;

        let account = acc_map_read_guard.acc_map.get(&acc).unwrap();

        let ephm_key_holder = &account.produce_ephemeral_key_holder();
        ephm_key_holder.log();

        let eph_pub_key =
            serde_json::to_vec(&ephm_key_holder.generate_ephemeral_public_key()).unwrap();

        let encoded_data = utxos
            .iter()
            .map(|utxo| {
                (
                    Account::encrypt_data(
                        &ephm_key_holder,
                        account.key_holder.viewing_public_key,
                        &serde_json::to_vec(&utxo).unwrap(),
                    ),
                    account.make_tag(),
                )
            })
            .map(|((ciphertext, nonce), tag)| (ciphertext, nonce.to_vec(), tag))
            .collect();

        let comm = generate_commitments(&utxos);

        let mint_multiple_utxo_addr_bytes: Vec<u8> =
            zkvm::test_methods::MINT_UTXO_MULTIPLE_ASSETS_ID
                .iter()
                .map(|num| num.to_le_bytes())
                .flatten()
                .collect();
        let sc_addr = hex::encode(mint_multiple_utxo_addr_bytes);

        //Sc does not change its state
        let state_changes: Vec<DataBlobChangeVariant> = vec![];
        let new_len = 0;
        let state_changes = (serde_json::to_value(state_changes).unwrap(), new_len);

        let sc_state = acc_map_read_guard
            .block_store
            .get_sc_sc_state(&sc_addr)
            .map_err(ExecutionFailureKind::db_error)?;

        let mut vec_values_u64: Vec<Vec<u64>> = sc_state
            .into_iter()
            .map(|slice| vec_u8_to_vec_u64(slice.to_vec()))
            .collect();

        let context = acc_map_read_guard.produce_context(account.address);

        //Will not panic, as PublicScContext is serializable
        let context_public_info: Vec<u64> = context.produce_u64_list_from_context().unwrap();
        vec_values_u64.push(context_public_info);

        let vec_public_info: Vec<u64> = vec_values_u64.into_iter().flatten().collect();

        let (tweak, secret_r, commitment) = pedersen_commitment_vec(vec_public_info);

        Ok((
            TransactionPayload {
                tx_kind: TxKind::Private,
                execution_input: vec![],
                execution_output: vec![],
                utxo_commitments_spent_hashes: vec![],
                utxo_commitments_created_hashes: comm
                    .into_iter()
                    .map(|hash_data| hash_data.try_into().unwrap())
                    .collect(),
                nullifier_created_hashes: vec![],
                execution_proof_private: sc_core::transaction_payloads_tools::encode_receipt(
                    receipt,
                )
                .unwrap(),
                encoded_data,
                ephemeral_pub_key: eph_pub_key.to_vec(),
                commitment,
                tweak,
                secret_r,
                sc_addr,
                state_changes,
            }
            .into(),
            result_hashes,
        ))
    }

    pub async fn transfer_utxo_private(
        &self,
        utxo: UTXO,
        commitment_in: [u8; 32],
        receivers: Vec<(u128, AccountAddress)>,
    ) -> Result<(Transaction, Vec<(AccountAddress, [u8; 32])>), ExecutionFailureKind> {
        let acc_map_read_guard = self.storage.read().await;

        let account = acc_map_read_guard.acc_map.get(&utxo.owner).unwrap();

        let nullifier = generate_nullifiers(
            &utxo,
            &account
                .key_holder
                .utxo_secret_key_holder
                .nullifier_secret_key
                .to_bytes()
                .to_vec(),
        );

        let (resulting_utxos, receipt) = prove_send_utxo(utxo, receivers)?;
        let utxo_hashes = resulting_utxos
            .iter()
            .map(|(utxo, addr)| (addr.clone(), utxo.hash))
            .collect();

        let utxos: Vec<UTXO> = resulting_utxos
            .iter()
            .map(|(utxo, _)| utxo.clone())
            .collect();

        let ephm_key_holder = &account.produce_ephemeral_key_holder();
        ephm_key_holder.log();

        let eph_pub_key =
            serde_json::to_vec(&ephm_key_holder.generate_ephemeral_public_key()).unwrap();

        let encoded_data: Vec<(Vec<u8>, Vec<u8>, u8)> = utxos
            .iter()
            .map(|utxo_enc| {
                let accout_enc = acc_map_read_guard.acc_map.get(&utxo_enc.owner).unwrap();

                let (ciphertext, nonce) = Account::encrypt_data(
                    &ephm_key_holder,
                    accout_enc.key_holder.viewing_public_key,
                    &serde_json::to_vec(&utxo_enc).unwrap(),
                );

                let tag = accout_enc.make_tag();

                (ciphertext, nonce.to_vec(), tag)
            })
            .collect();

        let commitments = generate_commitments(&utxos);

        let send_utxo_addr_bytes: Vec<u8> = zkvm::test_methods::SEND_UTXO_ID
            .iter()
            .map(|num| num.to_le_bytes())
            .flatten()
            .collect();
        let sc_addr = hex::encode(send_utxo_addr_bytes);

        //Sc does not change its state
        let state_changes: Vec<DataBlobChangeVariant> = vec![];
        let new_len = 0;
        let state_changes = (serde_json::to_value(state_changes).unwrap(), new_len);

        let sc_state = acc_map_read_guard
            .block_store
            .get_sc_sc_state(&sc_addr)
            .map_err(ExecutionFailureKind::db_error)?;

        let mut vec_values_u64: Vec<Vec<u64>> = sc_state
            .into_iter()
            .map(|slice| vec_u8_to_vec_u64(slice.to_vec()))
            .collect();

        let context = acc_map_read_guard.produce_context(account.address);

        //Will not panic, as PublicScContext is serializable
        let context_public_info: Vec<u64> = context.produce_u64_list_from_context().unwrap();
        vec_values_u64.push(context_public_info);

        let vec_public_info: Vec<u64> = vec_values_u64.into_iter().flatten().collect();

        let (tweak, secret_r, commitment) = pedersen_commitment_vec(vec_public_info);

        Ok((
            TransactionPayload {
                tx_kind: TxKind::Private,
                execution_input: vec![],
                execution_output: vec![],
                utxo_commitments_spent_hashes: vec![commitment_in],
                utxo_commitments_created_hashes: commitments
                    .into_iter()
                    .map(|hash_data| hash_data.try_into().unwrap())
                    .collect(),
                nullifier_created_hashes: vec![nullifier.try_into().unwrap()],
                execution_proof_private: sc_core::transaction_payloads_tools::encode_receipt(
                    receipt,
                )
                .unwrap(),
                encoded_data,
                ephemeral_pub_key: eph_pub_key.to_vec(),
                commitment,
                tweak,
                secret_r,
                sc_addr,
                state_changes,
            }
            .into(),
            utxo_hashes,
        ))
    }

    pub async fn transfer_utxo_multiple_assets_private(
        &self,
        utxos: Vec<UTXO>,
        commitments_in: Vec<[u8; 32]>,
        number_to_send: usize,
        receiver: AccountAddress,
    ) -> Result<(Transaction, Vec<[u8; 32]>, Vec<[u8; 32]>), ExecutionFailureKind> {
        let acc_map_read_guard = self.storage.read().await;

        let account = acc_map_read_guard.acc_map.get(&utxos[0].owner).unwrap();

        let nsk = account
            .key_holder
            .utxo_secret_key_holder
            .nullifier_secret_key
            .to_bytes()
            .to_vec();

        let nullifiers = utxos
            .iter()
            .map(|utxo| generate_nullifiers(utxo, &nsk))
            .map(|vecc| vecc.try_into().unwrap())
            .collect();

        let (resulting_utxos_receiver, resulting_utxos_not_spent, receipt) =
            prove_send_utxo_multiple_assets_one_receiver(utxos, number_to_send, receiver)?;

        let utxo_hashes_receiver = resulting_utxos_receiver
            .iter()
            .map(|utxo| utxo.hash)
            .collect();

        let utxo_hashes_not_spent = resulting_utxos_not_spent
            .iter()
            .map(|utxo| utxo.hash)
            .collect();

        let ephm_key_holder = &account.produce_ephemeral_key_holder();
        ephm_key_holder.log();

        let eph_pub_key =
            serde_json::to_vec(&ephm_key_holder.generate_ephemeral_public_key()).unwrap();

        let mut encoded_data: Vec<(Vec<u8>, Vec<u8>, u8)> = resulting_utxos_receiver
            .iter()
            .map(|utxo_enc| {
                let accout_enc = acc_map_read_guard.acc_map.get(&utxo_enc.owner).unwrap();

                let (ciphertext, nonce) = Account::encrypt_data(
                    &ephm_key_holder,
                    accout_enc.key_holder.viewing_public_key,
                    &serde_json::to_vec(&utxo_enc).unwrap(),
                );

                let tag = accout_enc.make_tag();

                (ciphertext, nonce.to_vec(), tag)
            })
            .collect();

        let encoded_data_1: Vec<(Vec<u8>, Vec<u8>, u8)> = resulting_utxos_not_spent
            .iter()
            .map(|utxo_enc| {
                let accout_enc = acc_map_read_guard.acc_map.get(&utxo_enc.owner).unwrap();

                let (ciphertext, nonce) = Account::encrypt_data(
                    &ephm_key_holder,
                    accout_enc.key_holder.viewing_public_key,
                    &serde_json::to_vec(&utxo_enc).unwrap(),
                );

                let tag = accout_enc.make_tag();

                (ciphertext, nonce.to_vec(), tag)
            })
            .collect();

        encoded_data.extend(encoded_data_1);

        let mut commitments = generate_commitments(&resulting_utxos_receiver);
        let commitments_1 = generate_commitments(&resulting_utxos_not_spent);

        commitments.extend(commitments_1);

        let send_multiple_utxo_addr_bytes: Vec<u8> =
            zkvm::test_methods::SEND_UTXO_MULTIPLE_ASSETS_ID
                .iter()
                .map(|num| num.to_le_bytes())
                .flatten()
                .collect();
        let sc_addr = hex::encode(send_multiple_utxo_addr_bytes);

        //Sc does not change its state
        let state_changes: Vec<DataBlobChangeVariant> = vec![];
        let new_len = 0;
        let state_changes = (serde_json::to_value(state_changes).unwrap(), new_len);

        let sc_state = acc_map_read_guard
            .block_store
            .get_sc_sc_state(&sc_addr)
            .map_err(ExecutionFailureKind::db_error)?;

        let mut vec_values_u64: Vec<Vec<u64>> = sc_state
            .into_iter()
            .map(|slice| vec_u8_to_vec_u64(slice.to_vec()))
            .collect();

        let context = acc_map_read_guard.produce_context(account.address);

        //Will not panic, as PublicScContext is serializable
        let context_public_info: Vec<u64> = context.produce_u64_list_from_context().unwrap();
        vec_values_u64.push(context_public_info);

        let vec_public_info: Vec<u64> = vec_values_u64.into_iter().flatten().collect();

        let (tweak, secret_r, commitment) = pedersen_commitment_vec(vec_public_info);

        Ok((
            TransactionPayload {
                tx_kind: TxKind::Private,
                execution_input: vec![],
                execution_output: vec![],
                utxo_commitments_spent_hashes: commitments_in,
                utxo_commitments_created_hashes: commitments
                    .into_iter()
                    .map(|hash_data| hash_data.try_into().unwrap())
                    .collect(),
                nullifier_created_hashes: nullifiers,
                execution_proof_private: sc_core::transaction_payloads_tools::encode_receipt(
                    receipt,
                )
                .unwrap(),
                encoded_data,
                ephemeral_pub_key: eph_pub_key.to_vec(),
                commitment,
                tweak,
                secret_r,
                sc_addr,
                state_changes,
            }
            .into(),
            utxo_hashes_receiver,
            utxo_hashes_not_spent,
        ))
    }

    pub async fn transfer_balance_shielded(
        &self,
        acc: AccountAddress,
        balance: u64,
        receivers: Vec<(u128, AccountAddress)>,
    ) -> Result<(Transaction, Vec<(AccountAddress, [u8; 32])>), ExecutionFailureKind> {
        let acc_map_read_guard = self.storage.read().await;

        let account = acc_map_read_guard.acc_map.get(&acc).unwrap();

        // TODO: add to transaction structure and do the check. Research has to update the scheme as well.
        let commitment = sc_core::transaction_payloads_tools::generate_secret_random_commitment(
            balance, account,
        )
        .unwrap();

        let nullifier = executions::se::generate_nullifiers(
            &commitment,
            &account
                .key_holder
                .utxo_secret_key_holder
                .nullifier_secret_key
                .to_bytes()
                .to_vec(),
        );

        let (resulting_utxos, receipt) = prove_send_utxo_shielded(acc, balance as u128, receivers)?;
        let utxo_hashes = resulting_utxos
            .iter()
            .map(|(utxo, addr)| (addr.clone(), utxo.hash))
            .collect();

        let utxos: Vec<UTXO> = resulting_utxos
            .iter()
            .map(|(utxo, _)| utxo.clone())
            .collect();

        let ephm_key_holder = &account.produce_ephemeral_key_holder();
        ephm_key_holder.log();

        let eph_pub_key =
            serde_json::to_vec(&ephm_key_holder.generate_ephemeral_public_key()).unwrap();

        let encoded_data: Vec<(Vec<u8>, Vec<u8>, u8)> = utxos
            .iter()
            .map(|utxo_enc| {
                let accout_enc = acc_map_read_guard.acc_map.get(&utxo_enc.owner).unwrap();

                let (ciphertext, nonce) = Account::encrypt_data(
                    &ephm_key_holder,
                    accout_enc.key_holder.viewing_public_key,
                    &serde_json::to_vec(&utxo_enc).unwrap(),
                );

                let tag = accout_enc.make_tag();

                (ciphertext, nonce.to_vec(), tag)
            })
            .collect();

        let commitments = generate_commitments(&utxos);

        let mint_utxo_addr_bytes: Vec<u8> = zkvm::test_methods::SEND_UTXO_ID
            .iter()
            .map(|num| num.to_le_bytes())
            .flatten()
            .collect();
        let sc_addr = hex::encode(mint_utxo_addr_bytes);

        //Sc does not change its state
        let state_changes: Vec<DataBlobChangeVariant> = vec![];
        let new_len = 0;
        let state_changes = (serde_json::to_value(state_changes).unwrap(), new_len);

        let sc_state = acc_map_read_guard
            .block_store
            .get_sc_sc_state(&sc_addr)
            .map_err(ExecutionFailureKind::db_error)?;

        let mut vec_values_u64: Vec<Vec<u64>> = sc_state
            .into_iter()
            .map(|slice| vec_u8_to_vec_u64(slice.to_vec()))
            .collect();

        let context = acc_map_read_guard.produce_context(account.address);

        //Will not panic, as PublicScContext is serializable
        let context_public_info: Vec<u64> = context.produce_u64_list_from_context().unwrap();
        vec_values_u64.push(context_public_info);

        let vec_public_info: Vec<u64> = vec_values_u64.into_iter().flatten().collect();

        let (tweak, secret_r, commitment) = pedersen_commitment_vec(vec_public_info);

        Ok((
            TransactionPayload {
                tx_kind: TxKind::Shielded,
                execution_input: serde_json::to_vec(&ActionData::SendMoneyShieldedTx(
                    SendMoneyShieldedTx {
                        acc_sender: acc,
                        amount: balance as u128,
                    },
                ))
                .unwrap(),
                execution_output: vec![],
                utxo_commitments_spent_hashes: vec![],
                utxo_commitments_created_hashes: commitments
                    .into_iter()
                    .map(|hash_data| hash_data.try_into().unwrap())
                    .collect(),
                nullifier_created_hashes: vec![nullifier.try_into().unwrap()],
                execution_proof_private: sc_core::transaction_payloads_tools::encode_receipt(
                    receipt,
                )
                .unwrap(),
                encoded_data,
                ephemeral_pub_key: eph_pub_key.to_vec(),
                commitment,
                tweak,
                secret_r,
                sc_addr,
                state_changes,
            }
            .into(),
            utxo_hashes,
        ))
    }

    pub async fn transfer_utxo_deshielded(
        &self,
        utxo: UTXO,
        comm_gen_hash: [u8; 32],
        receivers: Vec<(u128, AccountAddress)>,
    ) -> Result<Transaction, ExecutionFailureKind> {
        let acc_map_read_guard = self.storage.read().await;

        let commitment_in = acc_map_read_guard
            .utxo_commitments_store
            .get_tx(comm_gen_hash)
            .unwrap()
            .hash;

        let account = acc_map_read_guard.acc_map.get(&utxo.owner).unwrap();

        let nullifier = generate_nullifiers(
            &utxo,
            &account
                .key_holder
                .utxo_secret_key_holder
                .nullifier_secret_key
                .to_bytes()
                .to_vec(),
        );

        let (resulting_balances, receipt) = prove_send_utxo_deshielded(utxo, receivers)?;

        let send_utxo_addr_bytes: Vec<u8> = zkvm::test_methods::SEND_UTXO_ID
            .iter()
            .map(|num| num.to_le_bytes())
            .flatten()
            .collect();
        let sc_addr = hex::encode(send_utxo_addr_bytes);

        //Sc does not change its state
        let state_changes: Vec<DataBlobChangeVariant> = vec![];
        let new_len = 0;
        let state_changes = (serde_json::to_value(state_changes).unwrap(), new_len);

        let sc_state = acc_map_read_guard
            .block_store
            .get_sc_sc_state(&sc_addr)
            .map_err(ExecutionFailureKind::db_error)?;

        let mut vec_values_u64: Vec<Vec<u64>> = sc_state
            .into_iter()
            .map(|slice| vec_u8_to_vec_u64(slice.to_vec()))
            .collect();

        let context = acc_map_read_guard.produce_context(account.address);

        //Will not panic, as PublicScContext is serializable
        let context_public_info: Vec<u64> = context.produce_u64_list_from_context().unwrap();
        vec_values_u64.push(context_public_info);

        let vec_public_info: Vec<u64> = vec_values_u64.into_iter().flatten().collect();

        let (tweak, secret_r, commitment) = pedersen_commitment_vec(vec_public_info);

        Ok(TransactionPayload {
            tx_kind: TxKind::Deshielded,
            execution_input: serde_json::to_vec(&ActionData::SendMoneyDeshieldedTx(
                SendMoneyDeshieldedTx {
                    receiver_data: resulting_balances,
                },
            ))
            .unwrap(),
            execution_output: vec![],
            utxo_commitments_spent_hashes: vec![commitment_in],
            utxo_commitments_created_hashes: vec![],
            nullifier_created_hashes: vec![nullifier.try_into().unwrap()],
            execution_proof_private: sc_core::transaction_payloads_tools::encode_receipt(receipt)
                .unwrap(),
            encoded_data: vec![],
            ephemeral_pub_key: vec![],
            commitment,
            tweak,
            secret_r,
            sc_addr,
            state_changes,
        }
        .into())
    }

    pub async fn send_private_mint_tx(
        &self,
        acc: AccountAddress,
        amount: u128,
    ) -> Result<(SendTxResponse, [u8; 32], [u8; 32]), ExecutionFailureKind> {
        //Considering proof time, needs to be done before proof
        let tx_roots = self.get_roots().await;

        let point_before_prove = std::time::Instant::now();
        let (tx, utxo_hash) = self.mint_utxo_private(acc, amount).await?;
        tx.log();
        let point_after_prove = std::time::Instant::now();

        let commitment_generated_hash = tx.utxo_commitments_created_hashes[0];

        let timedelta = (point_after_prove - point_before_prove).as_millis();
        info!("Mint utxo proof spent {timedelta:?} milliseconds");

        Ok((
            self.sequencer_client.send_tx(tx, tx_roots).await?,
            utxo_hash,
            commitment_generated_hash,
        ))
    }

    pub async fn send_private_mint_multiple_assets_tx(
        &self,
        acc: AccountAddress,
        amount: u128,
        number_of_assets: usize,
    ) -> Result<(SendTxResponse, Vec<[u8; 32]>, Vec<[u8; 32]>), ExecutionFailureKind> {
        //Considering proof time, needs to be done before proof
        let tx_roots = self.get_roots().await;

        let point_before_prove = std::time::Instant::now();
        let (tx, utxo_hashes) = self
            .mint_utxo_multiple_assets_private(acc, amount, number_of_assets)
            .await?;
        tx.log();
        let point_after_prove = std::time::Instant::now();

        let commitment_generated_hashes = tx.utxo_commitments_created_hashes.clone();

        let timedelta = (point_after_prove - point_before_prove).as_millis();
        info!("Mint utxo proof spent {timedelta:?} milliseconds");

        Ok((
            self.sequencer_client.send_tx(tx, tx_roots).await?,
            utxo_hashes,
            commitment_generated_hashes,
        ))
    }

    pub async fn send_public_deposit(
        &self,
        acc: AccountAddress,
        amount: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        //Considering proof time, needs to be done before proof
        let tx_roots = self.get_roots().await;

        let public_context = {
            let read_guard = self.storage.read().await;

            read_guard.produce_context(acc)
        };

        let (tweak, secret_r, commitment) = pedersen_commitment_vec(
            //Will not panic, as public context is serializable
            public_context.produce_u64_list_from_context().unwrap(),
        );

        let sc_addr = hex::encode([0; 32]);

        //Sc does not change its state
        let state_changes: Vec<DataBlobChangeVariant> = vec![];
        let new_len = 0;
        let state_changes = (serde_json::to_value(state_changes).unwrap(), new_len);

        let tx: Transaction =
            sc_core::transaction_payloads_tools::create_public_transaction_payload(
                serde_json::to_vec(&ActionData::MintMoneyPublicTx(MintMoneyPublicTx {
                    acc,
                    amount,
                }))
                .unwrap(),
                commitment,
                tweak,
                secret_r,
                sc_addr,
                state_changes,
            )
            .into();
        tx.log();

        Ok(self.sequencer_client.send_tx(tx, tx_roots).await?)
    }

    pub async fn send_private_send_tx(
        &self,
        utxo: UTXO,
        comm_hash: [u8; 32],
        receivers: Vec<(u128, AccountAddress)>,
    ) -> Result<(SendTxResponse, Vec<([u8; 32], [u8; 32])>), ExecutionFailureKind> {
        //Considering proof time, needs to be done before proof
        let tx_roots = self.get_roots().await;

        let point_before_prove = std::time::Instant::now();
        let (tx, utxo_hashes) = self
            .transfer_utxo_private(utxo, comm_hash, receivers)
            .await?;
        tx.log();
        let point_after_prove = std::time::Instant::now();

        let timedelta = (point_after_prove - point_before_prove).as_millis();
        info!("Send private utxo proof spent {timedelta:?} milliseconds");

        Ok((
            self.sequencer_client.send_tx(tx, tx_roots).await?,
            utxo_hashes,
        ))
    }

    pub async fn send_private_multiple_assets_send_tx(
        &self,
        utxos: Vec<UTXO>,
        comm_hashes: Vec<[u8; 32]>,
        number_to_send: usize,
        receiver: AccountAddress,
    ) -> Result<(SendTxResponse, Vec<[u8; 32]>, Vec<[u8; 32]>), ExecutionFailureKind> {
        //Considering proof time, needs to be done before proof
        let tx_roots = self.get_roots().await;

        let point_before_prove = std::time::Instant::now();
        let (tx, utxo_hashes_received, utxo_hashes_not_spent) = self
            .transfer_utxo_multiple_assets_private(utxos, comm_hashes, number_to_send, receiver)
            .await?;
        tx.log();
        let point_after_prove = std::time::Instant::now();

        let timedelta = (point_after_prove - point_before_prove).as_millis();
        info!("Send private utxo proof spent {timedelta:?} milliseconds");

        Ok((
            self.sequencer_client.send_tx(tx, tx_roots).await?,
            utxo_hashes_received,
            utxo_hashes_not_spent,
        ))
    }

    pub async fn send_shielded_send_tx(
        &self,
        acc: AccountAddress,
        amount: u64,
        receivers: Vec<(u128, AccountAddress)>,
    ) -> Result<(SendTxResponse, Vec<([u8; 32], [u8; 32])>), ExecutionFailureKind> {
        //Considering proof time, needs to be done before proof
        let tx_roots = self.get_roots().await;

        let point_before_prove = std::time::Instant::now();
        let (tx, utxo_hashes) = self
            .transfer_balance_shielded(acc, amount, receivers)
            .await?;
        tx.log();
        let point_after_prove = std::time::Instant::now();

        let timedelta = (point_after_prove - point_before_prove).as_millis();
        info!("Send balance shielded proof spent {timedelta:?} milliseconds");

        Ok((
            self.sequencer_client.send_tx(tx, tx_roots).await?,
            utxo_hashes,
        ))
    }

    pub async fn send_deshielded_send_tx(
        &self,
        utxo: UTXO,
        comm_gen_hash: [u8; 32],
        receivers: Vec<(u128, AccountAddress)>,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        //Considering proof time, needs to be done before proof
        let tx_roots = self.get_roots().await;

        let point_before_prove = std::time::Instant::now();
        let tx = self
            .transfer_utxo_deshielded(utxo, comm_gen_hash, receivers)
            .await?;
        tx.log();
        let point_after_prove = std::time::Instant::now();

        let timedelta = (point_after_prove - point_before_prove).as_millis();
        info!("Send deshielded utxo proof spent {timedelta:?} milliseconds");

        Ok(self.sequencer_client.send_tx(tx, tx_roots).await?)
    }

    pub async fn operate_account_mint_private(
        &mut self,
        acc_addr: AccountAddress,
        amount: u128,
    ) -> Result<(UTXO, [u8; 32]), ExecutionFailureKind> {
        let (resp, new_utxo_hash, comm_gen_hash) =
            self.send_private_mint_tx(acc_addr, amount).await?;
        info!("Response for mint private is {resp:?}");

        info!("Awaiting new blocks");
        tokio::time::sleep(std::time::Duration::from_secs(BLOCK_GEN_DELAY_SECS)).await;

        let new_utxo = {
            let mut write_guard = self.storage.write().await;

            let acc = write_guard.acc_map.get_mut(&acc_addr).unwrap();

            acc.utxo_tree.get_item(new_utxo_hash)?.unwrap().clone()
        };

        new_utxo.log();
        info!(
            "Account address is {:?} ,new utxo owner address is {:?}",
            hex::encode(acc_addr),
            hex::encode(new_utxo.owner)
        );
        info!(
            "Account {:?} got new utxo with amount {amount:?}",
            hex::encode(acc_addr)
        );

        Ok((new_utxo, comm_gen_hash))
    }

    pub async fn operate_account_mint_multiple_assets_private(
        &mut self,
        acc_addr: AccountAddress,
        amount: u128,
        number_of_assets: usize,
    ) -> Result<(Vec<UTXO>, Vec<[u8; 32]>), ExecutionFailureKind> {
        let (resp, new_utxo_hashes, comm_gen_hashes) = self
            .send_private_mint_multiple_assets_tx(acc_addr, amount, number_of_assets)
            .await?;
        info!("Response for mint multiple assets private is {resp:?}");

        info!("Awaiting new blocks");
        tokio::time::sleep(std::time::Duration::from_secs(BLOCK_GEN_DELAY_SECS)).await;

        let new_utxos = {
            let mut write_guard = self.storage.write().await;

            new_utxo_hashes
                .into_iter()
                .map(|new_utxo_hash| {
                    let acc = write_guard.acc_map.get_mut(&acc_addr).unwrap();

                    let new_utxo = acc
                        .utxo_tree
                        .get_item(new_utxo_hash)
                        .unwrap()
                        .unwrap()
                        .clone();

                    new_utxo.log();
                    info!(
                        "Account address is {:?} ,new utxo owner address is {:?}",
                        hex::encode(acc_addr),
                        hex::encode(new_utxo.owner)
                    );
                    info!(
                        "Account {:?} got new utxo with amount {amount:?} and asset {:?}",
                        hex::encode(acc_addr),
                        new_utxo.asset
                    );

                    new_utxo
                })
                .collect()
        };

        Ok((new_utxos, comm_gen_hashes))
    }

    pub async fn operate_account_send_deshielded_one_receiver(
        &mut self,
        acc_addr_rec: AccountAddress,
        utxo: UTXO,
        comm_gen_hash: [u8; 32],
    ) -> Result<(), ExecutionFailureKind> {
        let amount = utxo.amount;

        let old_balance = {
            let acc_map_read_guard = self.storage.read().await;

            let acc = acc_map_read_guard.acc_map.get(&acc_addr_rec).unwrap();

            acc.balance
        };

        info!(
            "Balance of receiver {:?} now is {old_balance:?}",
            hex::encode(acc_addr_rec)
        );

        let resp = self
            .send_deshielded_send_tx(utxo, comm_gen_hash, vec![(amount, acc_addr_rec)])
            .await?;
        info!("Response for send deshielded is {resp:?}");

        info!("Awaiting new blocks");
        tokio::time::sleep(std::time::Duration::from_secs(BLOCK_GEN_DELAY_SECS)).await;

        let new_balance = {
            let acc_map_read_guard = self.storage.read().await;

            let acc = acc_map_read_guard.acc_map.get(&acc_addr_rec).unwrap();

            acc.balance
        };

        info!(
            "Balance of receiver {:?} now is {:?}, delta is {:?}",
            hex::encode(acc_addr_rec),
            new_balance,
            new_balance - old_balance
        );

        Ok(())
    }

    pub async fn operate_account_deposit_public(
        &mut self,
        acc_addr: AccountAddress,
        amount: u128,
    ) -> Result<(), ExecutionFailureKind> {
        let old_balance = {
            let acc_map_read_guard = self.storage.read().await;

            let acc = acc_map_read_guard.acc_map.get(&acc_addr).unwrap();

            acc.balance
        };

        info!(
            "Balance of {:?} now is {old_balance:?}",
            hex::encode(acc_addr)
        );

        let resp = self.send_public_deposit(acc_addr, amount).await?;
        info!("Response for public deposit is {resp:?}");

        info!("Awaiting new blocks");
        tokio::time::sleep(std::time::Duration::from_secs(BLOCK_GEN_DELAY_SECS)).await;

        let new_balance = {
            let acc_map_read_guard = self.storage.read().await;

            let acc = acc_map_read_guard.acc_map.get(&acc_addr).unwrap();

            acc.balance
        };

        info!(
            "Balance of {:?} now is {new_balance:?}, delta is {:?}",
            hex::encode(acc_addr),
            new_balance - old_balance
        );

        Ok(())
    }

    pub async fn operate_account_send_shielded_one_receiver(
        &mut self,
        acc_addr_sender: AccountAddress,
        acc_addr_rec: AccountAddress,
        amount: u128,
    ) -> Result<UTXO, ExecutionFailureKind> {
        let (resp, new_utxo_hashes) = self
            .send_shielded_send_tx(acc_addr_sender, amount as u64, vec![(amount, acc_addr_rec)])
            .await?;
        info!("Response for send shielded is {resp:?}");

        let new_utxo_hash = new_utxo_hashes[0].1;

        info!("Awaiting new blocks");
        tokio::time::sleep(std::time::Duration::from_secs(BLOCK_GEN_DELAY_SECS)).await;

        let new_utxo = {
            let mut write_guard = self.storage.write().await;

            let acc = write_guard.acc_map.get_mut(&acc_addr_rec).unwrap();
            acc.log();

            acc.utxo_tree.get_item(new_utxo_hash)?.unwrap().clone()
        };
        new_utxo.log();
        info!(
            "Account address is {:?} ,new utxo owner address is {:?}",
            hex::encode(acc_addr_rec),
            hex::encode(new_utxo.owner)
        );
        info!(
            "Account {:?} got new utxo with amount {amount:?}",
            hex::encode(acc_addr_rec)
        );

        Ok(new_utxo)
    }

    pub async fn operate_account_send_private_one_receiver(
        &mut self,
        acc_addr_rec: AccountAddress,
        utxo: UTXO,
        comm_gen_hash: [u8; 32],
    ) -> Result<UTXO, ExecutionFailureKind> {
        let amount = utxo.amount;

        let (resp, new_utxo_hashes) = self
            .send_private_send_tx(utxo, comm_gen_hash, vec![(amount, acc_addr_rec)])
            .await?;
        info!("Response for send private is {resp:?}");

        let new_utxo_hash = new_utxo_hashes[0].1;

        info!("Awaiting new blocks");
        tokio::time::sleep(std::time::Duration::from_secs(BLOCK_GEN_DELAY_SECS)).await;

        let new_utxo = {
            let mut write_guard = self.storage.write().await;

            let acc = write_guard.acc_map.get_mut(&acc_addr_rec).unwrap();
            acc.log();

            acc.utxo_tree.get_item(new_utxo_hash)?.unwrap().clone()
        };
        new_utxo.log();
        info!(
            "Account address is {:?} ,new utxo owner address is {:?}",
            hex::encode(acc_addr_rec),
            hex::encode(new_utxo.owner)
        );
        info!(
            "Account {:?} got new utxo with amount {:?}",
            hex::encode(acc_addr_rec),
            new_utxo.amount
        );

        Ok(new_utxo)
    }

    pub async fn operate_account_send_private_multiple_assets_one_receiver(
        &mut self,
        acc_addr: AccountAddress,
        acc_addr_rec: AccountAddress,
        utxos: Vec<UTXO>,
        comm_gen_hashes: Vec<[u8; 32]>,
        number_to_send: usize,
    ) -> Result<(), ExecutionFailureKind> {
        let (resp, new_utxo_hashes_rec, new_utxo_hashes_not_sp) = self
            .send_private_multiple_assets_send_tx(
                utxos,
                comm_gen_hashes,
                number_to_send,
                acc_addr_rec,
            )
            .await?;
        info!("Response for send private multiple assets is {resp:?}");

        info!("Awaiting new blocks");
        tokio::time::sleep(std::time::Duration::from_secs(BLOCK_GEN_DELAY_SECS)).await;

        {
            let mut write_guard = self.storage.write().await;

            for new_utxo_hash in new_utxo_hashes_rec {
                let acc = write_guard.acc_map.get_mut(&acc_addr_rec).unwrap();
                acc.log();

                let new_utxo = acc.utxo_tree.get_item(new_utxo_hash)?.unwrap().clone();

                new_utxo.log();
                info!(
                    "Account address is {:?} ,new utxo owner address is {:?}",
                    hex::encode(acc_addr_rec),
                    hex::encode(new_utxo.owner)
                );
                info!(
                    "Account {:?} got new utxo with amount {:?} and asset {:?}",
                    hex::encode(acc_addr_rec),
                    new_utxo.amount,
                    new_utxo.asset,
                );
            }

            for new_utxo_hash in new_utxo_hashes_not_sp {
                let acc = write_guard.acc_map.get_mut(&acc_addr).unwrap();
                acc.log();

                let new_utxo = acc.utxo_tree.get_item(new_utxo_hash)?.unwrap().clone();

                new_utxo.log();
                info!(
                    "Account address is {:?} ,new utxo owner address is {:?}",
                    hex::encode(acc_addr),
                    hex::encode(new_utxo.owner)
                );
                info!(
                    "Account {:?} got new utxo with amount {:?} and asset {:?}",
                    hex::encode(acc_addr),
                    new_utxo.amount,
                    new_utxo.asset,
                );
            }
        }

        Ok(())
    }

    pub async fn split_utxo(
        &self,
        utxo: UTXO,
        commitment_in: [u8; 32],
        receivers: Vec<(u128, AccountAddress)>,
        visibility_list: [bool; 3],
    ) -> Result<(Transaction, Vec<(AccountAddress, [u8; 32])>), ExecutionFailureKind> {
        let acc_map_read_guard = self.storage.read().await;

        let account = acc_map_read_guard.acc_map.get(&utxo.owner).unwrap();

        let nullifier = generate_nullifiers(
            &utxo,
            &account
                .key_holder
                .utxo_secret_key_holder
                .nullifier_secret_key
                .to_bytes()
                .to_vec(),
        );

        let (resulting_utxos, receipt) = prove_send_utxo(utxo, receivers)?;
        let utxo_hashes = resulting_utxos
            .iter()
            .map(|(utxo, addr)| (addr.clone(), utxo.hash))
            .collect();

        let utxos: Vec<UTXO> = resulting_utxos
            .iter()
            .map(|(utxo, _)| utxo.clone())
            .collect();

        let ephm_key_holder = &account.produce_ephemeral_key_holder();
        ephm_key_holder.log();

        let eph_pub_key =
            serde_json::to_vec(&ephm_key_holder.generate_ephemeral_public_key()).unwrap();

        let encoded_data: Vec<(Vec<u8>, Vec<u8>, u8)> = utxos
            .iter()
            .map(|utxo_enc| {
                let accout_enc = acc_map_read_guard.acc_map.get(&utxo_enc.owner).unwrap();

                let (ciphertext, nonce) = Account::encrypt_data(
                    &ephm_key_holder,
                    accout_enc.key_holder.viewing_public_key,
                    &serde_json::to_vec(&utxo_enc).unwrap(),
                );

                let tag = accout_enc.make_tag();

                (ciphertext, nonce.to_vec(), tag)
            })
            .collect();

        let commitments = generate_commitments(&utxos);

        let publication = ActionData::UTXOPublication(UTXOPublication {
            utxos: utxos
                .iter()
                .enumerate()
                .filter_map(|(id, item)| {
                    if visibility_list[id] {
                        Some(item.clone())
                    } else {
                        None
                    }
                })
                .collect(),
        });

        let send_utxo_addr_bytes: Vec<u8> = zkvm::test_methods::SEND_UTXO_ID
            .iter()
            .map(|num| num.to_le_bytes())
            .flatten()
            .collect();
        let sc_addr = hex::encode(send_utxo_addr_bytes);

        //Sc does not change its state
        let state_changes: Vec<DataBlobChangeVariant> = vec![];
        let new_len = 0;
        let state_changes = (serde_json::to_value(state_changes).unwrap(), new_len);

        let sc_state = acc_map_read_guard
            .block_store
            .get_sc_sc_state(&sc_addr)
            .map_err(ExecutionFailureKind::db_error)?;

        let mut vec_values_u64: Vec<Vec<u64>> = sc_state
            .into_iter()
            .map(|slice| vec_u8_to_vec_u64(slice.to_vec()))
            .collect();

        let context = acc_map_read_guard.produce_context(account.address);

        //Will not panic, as PublicScContext is serializable
        let context_public_info: Vec<u64> = context.produce_u64_list_from_context().unwrap();
        vec_values_u64.push(context_public_info);

        let vec_public_info: Vec<u64> = vec_values_u64.into_iter().flatten().collect();

        let (tweak, secret_r, commitment) = pedersen_commitment_vec(vec_public_info);

        Ok((
            TransactionPayload {
                tx_kind: TxKind::Shielded,
                execution_input: vec![],
                execution_output: serde_json::to_vec(&publication).unwrap(),
                utxo_commitments_spent_hashes: vec![commitment_in],
                utxo_commitments_created_hashes: commitments
                    .clone()
                    .into_iter()
                    .map(|hash_data| hash_data.try_into().unwrap())
                    .collect(),
                nullifier_created_hashes: vec![nullifier.try_into().unwrap()],
                execution_proof_private: sc_core::transaction_payloads_tools::encode_receipt(
                    receipt,
                )
                .unwrap(),
                encoded_data,
                ephemeral_pub_key: eph_pub_key.to_vec(),
                commitment,
                tweak,
                secret_r,
                sc_addr,
                state_changes,
            }
            .into(),
            utxo_hashes,
        ))
    }

    pub async fn send_split_tx(
        &self,
        utxo: UTXO,
        comm_hash: [u8; 32],
        receivers: Vec<(u128, AccountAddress)>,
        visibility_list: [bool; 3],
    ) -> Result<(SendTxResponse, Vec<([u8; 32], [u8; 32])>, Vec<[u8; 32]>), ExecutionFailureKind>
    {
        //Considering proof time, needs to be done before proof
        let tx_roots = self.get_roots().await;

        let point_before_prove = std::time::Instant::now();
        let (tx, utxo_hashes) = self
            .split_utxo(utxo, comm_hash, receivers, visibility_list)
            .await?;
        tx.log();
        let point_after_prove = std::time::Instant::now();

        let timedelta = (point_after_prove - point_before_prove).as_millis();
        info!("Send private utxo proof spent {timedelta:?} milliseconds");

        let commitments = tx.utxo_commitments_created_hashes.clone();

        Ok((
            self.sequencer_client.send_tx(tx, tx_roots).await?,
            utxo_hashes,
            commitments,
        ))
    }

    pub async fn operate_account_send_split_utxo(
        &mut self,
        addrs_receivers: [AccountAddress; 3],
        utxo: UTXO,
        comm_gen_hash: [u8; 32],
        visibility_list: [bool; 3],
    ) -> Result<(Vec<UTXO>, Vec<[u8; 32]>), ExecutionFailureKind> {
        let (resp, new_utxo_hashes, commitments_hashes) = self
            .send_split_tx(
                utxo.clone(),
                comm_gen_hash,
                addrs_receivers
                    .clone()
                    .map(|addr| (utxo.amount / 3, addr))
                    .to_vec(),
                visibility_list,
            )
            .await?;
        info!("Response for send shielded is {resp:?}");

        info!("Awaiting new blocks");
        tokio::time::sleep(std::time::Duration::from_secs(BLOCK_GEN_DELAY_SECS)).await;

        let new_utxos: Vec<UTXO> = {
            let mut write_guard = self.storage.write().await;

            new_utxo_hashes
                .into_iter()
                .map(|(acc_addr_rec, new_utxo_hash)| {
                    let acc = write_guard.acc_map.get_mut(&acc_addr_rec).unwrap();

                    let new_utxo = acc
                        .utxo_tree
                        .get_item(new_utxo_hash)
                        .unwrap()
                        .unwrap()
                        .clone();
                    new_utxo.log();

                    info!(
                        "Account address is {:?} ,new utxo owner address is {:?}",
                        hex::encode(acc_addr_rec),
                        hex::encode(new_utxo.owner)
                    );
                    info!(
                        "Account {:?} got new utxo with amount {:?}",
                        hex::encode(acc_addr_rec),
                        new_utxo.amount
                    );

                    new_utxo
                })
                .collect()
        };

        Ok((new_utxos, commitments_hashes))
    }

    ///Mint utxo, make it public
    pub async fn subscenario_1(&mut self) -> Result<(), ExecutionFailureKind> {
        let acc_addr = self.create_new_account().await;

        let (new_utxo, comm_gen_hash) = self.operate_account_mint_private(acc_addr, 100).await?;

        self.operate_account_send_deshielded_one_receiver(acc_addr, new_utxo, comm_gen_hash)
            .await?;

        Ok(())
    }

    ///Deposit balance, make it private
    pub async fn subscenario_2(&mut self) -> Result<(), ExecutionFailureKind> {
        let acc_addr = self.create_new_account().await;

        self.operate_account_deposit_public(acc_addr, 100).await?;

        self.operate_account_send_shielded_one_receiver(acc_addr, acc_addr, 100)
            .await?;

        Ok(())
    }

    ///Mint utxo, privately send it to another user
    pub async fn subscenario_3(&mut self) -> Result<(), ExecutionFailureKind> {
        let acc_addr = self.create_new_account().await;
        let acc_addr_rec = self.create_new_account().await;

        let (new_utxo, comm_gen_hash) = self.operate_account_mint_private(acc_addr, 100).await?;

        self.operate_account_send_private_one_receiver(acc_addr_rec, new_utxo, comm_gen_hash)
            .await?;

        Ok(())
    }

    ///Deposit balance, shielded send it to another user
    pub async fn subscenario_4(&mut self) -> Result<(), ExecutionFailureKind> {
        let acc_addr = self.create_new_account().await;
        let acc_addr_rec = self.create_new_account().await;

        self.operate_account_deposit_public(acc_addr, 100).await?;

        self.operate_account_send_shielded_one_receiver(acc_addr, acc_addr_rec, 100)
            .await?;

        Ok(())
    }

    ///Mint utxo, deshielded send it to another user
    pub async fn subscenario_5(&mut self) -> Result<(), ExecutionFailureKind> {
        let acc_addr = self.create_new_account().await;
        let acc_addr_rec = self.create_new_account().await;

        let (new_utxo, comm_gen_hash) = self.operate_account_mint_private(acc_addr, 100).await?;

        self.operate_account_send_deshielded_one_receiver(acc_addr_rec, new_utxo, comm_gen_hash)
            .await?;

        Ok(())
    }

    ///First complex scenario.
    /// Creating accounts A, B, C, D.
    /// Minting UTXO for A, splitting it between B, C, D.
    /// Variable `visibility_list` decides, which of actions will be visible on blockchain.
    /// Variable `publication index` decides, who of B, C or D moves its UTXO into public state.
    pub async fn scenario_1(
        &mut self,
        visibility_list: [bool; 3],
        publication_index: usize,
    ) -> Result<(), ExecutionFailureKind> {
        let acc_addr_sender = self.create_new_account().await;

        let acc_addr_rec_1 = self.create_new_account().await;
        let acc_addr_rec_2 = self.create_new_account().await;
        let acc_addr_rec_3 = self.create_new_account().await;

        let addrs_receivers = [acc_addr_rec_1, acc_addr_rec_2, acc_addr_rec_3];

        let (new_utxo, comm_gen_hash) = self
            .operate_account_mint_private(acc_addr_sender, 99)
            .await?;

        let (new_utxos, comm_gen_hashes) = self
            .operate_account_send_split_utxo(
                addrs_receivers,
                new_utxo,
                comm_gen_hash,
                visibility_list,
            )
            .await?;

        self.operate_account_send_deshielded_one_receiver(
            addrs_receivers[publication_index],
            new_utxos[publication_index].clone(),
            comm_gen_hashes[publication_index],
        )
        .await?;

        Ok(())
    }

    ///Mint number of different assets with same amount for account
    pub async fn scenario_2(
        &mut self,
        number_of_assets: usize,
        number_to_send: usize,
    ) -> Result<(), ExecutionFailureKind> {
        let acc_addr_sender = self.create_new_account().await;
        let acc_addr_receiver = self.create_new_account().await;

        let (utxos, comm_gen_hashes) = self
            .operate_account_mint_multiple_assets_private(acc_addr_sender, 100, number_of_assets)
            .await?;

        self.operate_account_send_private_multiple_assets_one_receiver(
            acc_addr_sender,
            acc_addr_receiver,
            utxos,
            comm_gen_hashes,
            number_to_send,
        )
        .await?;

        Ok(())
    }
}

pub fn generate_commitments_helper(input_utxos: &[UTXO]) -> Vec<[u8; 32]> {
    generate_commitments(input_utxos)
        .into_iter()
        .map(|comm_raw| comm_raw.try_into().unwrap())
        .collect()
}

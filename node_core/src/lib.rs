use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use k256::elliptic_curve::group::GroupEncoding;

use ::storage::transaction::{Transaction, TransactionPayload, TxKind};
use accounts::account_core::{Account, AccountAddress};
use anyhow::Result;
use config::NodeConfig;
use executions::{
    private_exec::{generate_commitments, generate_nullifiers},
    se::{commit, tag_random},
};
use log::info;
use rand::thread_rng;
use secp256k1_zkp::{CommitmentSecrets, Tweak};
use sequencer_client::{json::SendTxResponse, SequencerClient};
use serde::{Deserialize, Serialize};
use storage::NodeChainStore;
use tokio::{sync::RwLock, task::JoinHandle};
use utxo::utxo_core::UTXO;
use zkvm::{
    prove_mint_utxo, prove_send_utxo, prove_send_utxo_deshielded, prove_send_utxo_shielded,
};

pub const BLOCK_GEN_DELAY_SECS: u64 = 20;

pub mod config;
pub mod executions;
pub mod sequencer_client;
pub mod storage;

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
pub enum ActionData {
    MintMoneyPublicTx(MintMoneyPublicTx),
    SendMoneyShieldedTx(SendMoneyShieldedTx),
    SendMoneyDeshieldedTx(SendMoneyDeshieldedTx),
}

pub struct NodeCore {
    pub storage: Arc<RwLock<NodeChainStore>>,
    pub curr_height: Arc<AtomicU64>,
    pub node_config: NodeConfig,
    pub db_updater_handle: JoinHandle<Result<()>>,
    pub sequencer_client: Arc<SequencerClient>,
}

impl NodeCore {
    pub async fn start_from_config_update_chain(config: NodeConfig) -> Result<Self> {
        let client = Arc::new(SequencerClient::new(config.clone())?);

        let genesis_id = client.get_genesis_id().await?;
        info!("Gesesis id is {genesis_id:?}");

        let genesis_block = client.get_block(genesis_id.genesis_id).await?.block;

        let mut storage = NodeChainStore::new_with_genesis(&config.home, genesis_block);

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
        })
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
    ) -> (Transaction, [u8; 32]) {
        let (utxo, receipt) = prove_mint_utxo(amount, acc);
        let result_hash = utxo.hash;

        let acc_map_read_guard = self.storage.read().await;

        let accout = acc_map_read_guard.acc_map.get(&acc).unwrap();
        accout.log();

        let ephm_key_holder = &accout.produce_ephemeral_key_holder();

        let eph_pub_key = ephm_key_holder.generate_ephemeral_public_key().to_bytes();

        let encoded_data = Account::encrypt_data(
            &ephm_key_holder,
            accout.key_holder.viewing_public_key,
            &serde_json::to_vec(&utxo).unwrap(),
        );

        let comm = generate_commitments(&vec![utxo]);

        (
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
                execution_proof_private: serde_json::to_string(&receipt).unwrap(),
                encoded_data: vec![(encoded_data.0, encoded_data.1.to_vec())],
                ephemeral_pub_key: eph_pub_key.to_vec(),
            }
            .into(),
            result_hash,
        )
    }

    pub fn deposit_money_public(&self, acc: AccountAddress, amount: u128) -> Transaction {
        TransactionPayload {
            tx_kind: TxKind::Public,
            execution_input: serde_json::to_vec(&ActionData::MintMoneyPublicTx(
                MintMoneyPublicTx { acc, amount },
            ))
            .unwrap(),
            execution_output: vec![],
            utxo_commitments_spent_hashes: vec![],
            utxo_commitments_created_hashes: vec![],
            nullifier_created_hashes: vec![],
            execution_proof_private: "".to_string(),
            encoded_data: vec![],
            ephemeral_pub_key: vec![],
        }
        .into()
    }

    pub async fn transfer_utxo_private(
        &self,
        utxo: UTXO,
        commitment_in: [u8; 32],
        receivers: Vec<(u128, AccountAddress)>,
    ) -> (Transaction, Vec<(AccountAddress, [u8; 32])>) {
        let acc_map_read_guard = self.storage.read().await;

        let accout = acc_map_read_guard.acc_map.get(&utxo.owner).unwrap();
        accout.log();

        let nullifier = generate_nullifiers(
            &utxo,
            &accout
                .key_holder
                .utxo_secret_key_holder
                .nullifier_secret_key
                .to_bytes()
                .to_vec(),
        );

        let (resulting_utxos, receipt) = prove_send_utxo(utxo, receivers);
        let utxo_hashes = resulting_utxos
            .iter()
            .map(|(utxo, addr)| (addr.clone(), utxo.hash))
            .collect();

        let utxos: Vec<UTXO> = resulting_utxos
            .iter()
            .map(|(utxo, _)| utxo.clone())
            .collect();

        let ephm_key_holder = &accout.produce_ephemeral_key_holder();

        let eph_pub_key = ephm_key_holder.generate_ephemeral_public_key().to_bytes();

        let encoded_data: Vec<(Vec<u8>, Vec<u8>)> = utxos
            .iter()
            .map(|utxo_enc| {
                let accout_enc = acc_map_read_guard.acc_map.get(&utxo_enc.owner).unwrap();

                let (ciphertext, nonce) = Account::encrypt_data(
                    &ephm_key_holder,
                    accout_enc.key_holder.viewing_public_key,
                    &serde_json::to_vec(&utxo_enc).unwrap(),
                );

                (ciphertext, nonce.to_vec())
            })
            .collect();

        let commitments = generate_commitments(&utxos);

        (
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
                execution_proof_private: serde_json::to_string(&receipt).unwrap(),
                encoded_data,
                ephemeral_pub_key: eph_pub_key.to_vec(),
            }
            .into(),
            utxo_hashes,
        )
    }

    pub async fn transfer_balance_shielded(
        &self,
        acc: AccountAddress,
        balance: u64,
        receivers: Vec<(u128, AccountAddress)>,
    ) -> (Transaction, Vec<(AccountAddress, [u8; 32])>) {
        let acc_map_read_guard = self.storage.read().await;

        let accout = acc_map_read_guard.acc_map.get(&acc).unwrap();
        accout.log();

        let commitment_secrets = CommitmentSecrets {
            value: balance,
            value_blinding_factor: Tweak::from_slice(
                &accout
                    .key_holder
                    .utxo_secret_key_holder
                    .viewing_secret_key
                    .to_bytes()
                    .to_vec(),
            )
            .unwrap(),
            generator_blinding_factor: Tweak::new(&mut thread_rng()),
        };

        let tag = tag_random();
        let commitment = commit(&commitment_secrets, tag);

        let nullifier = executions::se::generate_nullifiers(
            &commitment,
            &accout
                .key_holder
                .utxo_secret_key_holder
                .nullifier_secret_key
                .to_bytes()
                .to_vec(),
        );

        let (resulting_utxos, receipt) = prove_send_utxo_shielded(acc, balance as u128, receivers);
        let utxo_hashes = resulting_utxos
            .iter()
            .map(|(utxo, addr)| (addr.clone(), utxo.hash))
            .collect();

        let utxos: Vec<UTXO> = resulting_utxos
            .iter()
            .map(|(utxo, _)| utxo.clone())
            .collect();

        let ephm_key_holder = &accout.produce_ephemeral_key_holder();

        let eph_pub_key = ephm_key_holder.generate_ephemeral_public_key().to_bytes();

        let encoded_data: Vec<(Vec<u8>, Vec<u8>)> = utxos
            .iter()
            .map(|utxo_enc| {
                let accout_enc = acc_map_read_guard.acc_map.get(&utxo_enc.owner).unwrap();

                let (ciphertext, nonce) = Account::encrypt_data(
                    &ephm_key_holder,
                    accout_enc.key_holder.viewing_public_key,
                    &serde_json::to_vec(&utxo_enc).unwrap(),
                );

                (ciphertext, nonce.to_vec())
            })
            .collect();

        let commitments = generate_commitments(&utxos);

        (
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
                execution_proof_private: serde_json::to_string(&receipt).unwrap(),
                encoded_data,
                ephemeral_pub_key: eph_pub_key.to_vec(),
            }
            .into(),
            utxo_hashes,
        )
    }

    pub async fn transfer_utxo_deshielded(
        &self,
        utxo: UTXO,
        comm_gen_hash: [u8; 32],
        receivers: Vec<(u128, AccountAddress)>,
    ) -> Transaction {
        let acc_map_read_guard = self.storage.read().await;

        let commitment_in = acc_map_read_guard
            .utxo_commitments_store
            .get_tx(comm_gen_hash)
            .unwrap()
            .hash;

        let accout = acc_map_read_guard.acc_map.get(&utxo.owner).unwrap();
        accout.log();

        let nullifier = generate_nullifiers(
            &utxo,
            &accout
                .key_holder
                .utxo_secret_key_holder
                .nullifier_secret_key
                .to_bytes()
                .to_vec(),
        );

        let (resulting_balances, receipt) = prove_send_utxo_deshielded(utxo, receivers);

        TransactionPayload {
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
            execution_proof_private: serde_json::to_string(&receipt).unwrap(),
            encoded_data: vec![],
            ephemeral_pub_key: vec![],
        }
        .into()
    }

    pub async fn send_private_mint_tx(
        &self,
        acc: AccountAddress,
        amount: u128,
    ) -> Result<(SendTxResponse, [u8; 32], [u8; 32])> {
        let point_before_prove = std::time::Instant::now();
        let (tx, utxo_hash) = self.mint_utxo_private(acc, amount).await;
        tx.log();
        let point_after_prove = std::time::Instant::now();

        let commitment_generated_hash = tx.utxo_commitments_created_hashes[0];

        let timedelta = (point_after_prove - point_before_prove).as_millis();
        info!("Mint utxo proof spent {timedelta:?} milliseconds");

        Ok((
            self.sequencer_client.send_tx(tx).await?,
            utxo_hash,
            commitment_generated_hash,
        ))
    }

    pub async fn send_public_deposit(
        &self,
        acc: AccountAddress,
        amount: u128,
    ) -> Result<SendTxResponse> {
        Ok(self
            .sequencer_client
            .send_tx(self.deposit_money_public(acc, amount))
            .await?)
    }

    pub async fn send_private_send_tx(
        &self,
        utxo: UTXO,
        comm_hash: [u8; 32],
        receivers: Vec<(u128, AccountAddress)>,
    ) -> Result<(SendTxResponse, Vec<([u8; 32], [u8; 32])>)> {
        let point_before_prove = std::time::Instant::now();
        let (tx, utxo_hashes) = self.transfer_utxo_private(utxo, comm_hash, receivers).await;
        tx.log();
        let point_after_prove = std::time::Instant::now();

        let timedelta = (point_after_prove - point_before_prove).as_millis();
        info!("Send private utxo proof spent {timedelta:?} milliseconds");

        Ok((self.sequencer_client.send_tx(tx).await?, utxo_hashes))
    }

    pub async fn send_shielded_send_tx(
        &self,
        acc: AccountAddress,
        amount: u64,
        receivers: Vec<(u128, AccountAddress)>,
    ) -> Result<(SendTxResponse, Vec<([u8; 32], [u8; 32])>)> {
        let point_before_prove = std::time::Instant::now();
        let (tx, utxo_hashes) = self.transfer_balance_shielded(acc, amount, receivers).await;
        tx.log();
        let point_after_prove = std::time::Instant::now();

        let timedelta = (point_after_prove - point_before_prove).as_millis();
        info!("Send balance shielded proof spent {timedelta:?} milliseconds");

        Ok((self.sequencer_client.send_tx(tx).await?, utxo_hashes))
    }

    pub async fn send_deshielded_send_tx(
        &self,
        utxo: UTXO,
        comm_gen_hash: [u8; 32],
        receivers: Vec<(u128, AccountAddress)>,
    ) -> Result<SendTxResponse> {
        let point_before_prove = std::time::Instant::now();
        let tx = self
            .transfer_utxo_deshielded(utxo, comm_gen_hash, receivers)
            .await;
        tx.log();
        let point_after_prove = std::time::Instant::now();

        let timedelta = (point_after_prove - point_before_prove).as_millis();
        info!("Send deshielded utxo proof spent {timedelta:?} milliseconds");

        Ok(self.sequencer_client.send_tx(tx).await?)
    }

    ///Mint utxo, make it public
    pub async fn subscenario_1(&mut self) {
        let acc_addr = self.create_new_account().await;
        info!("Account created {acc_addr:?}");

        let (resp, new_utxo_hash, comm_gen_hash) =
            self.send_private_mint_tx(acc_addr, 100).await.unwrap();
        info!("Response for mint private is {resp:?}");

        info!("Awaiting new blocks");
        tokio::time::sleep(std::time::Duration::from_secs(BLOCK_GEN_DELAY_SECS)).await;

        let new_utxo = {
            let mut write_guard = self.storage.write().await;

            let acc = write_guard.acc_map.get_mut(&acc_addr).unwrap();
            acc.log();

            acc.utxo_tree
                .get_item(new_utxo_hash)
                .unwrap()
                .unwrap()
                .clone()
        };

        new_utxo.log();

        let resp = self
            .send_deshielded_send_tx(new_utxo, comm_gen_hash, vec![(100, acc_addr)])
            .await
            .unwrap();
        info!("Response for send deshielded is {resp:?}");

        info!("Awaiting new blocks");
        tokio::time::sleep(std::time::Duration::from_secs(BLOCK_GEN_DELAY_SECS)).await;

        let new_balance = {
            let acc_map_read_guard = self.storage.read().await;

            let acc = acc_map_read_guard.acc_map.get(&acc_addr).unwrap();
            acc.log();

            acc.balance
        };

        info!("New account public balance is {new_balance:?}");
    }

    ///Deposit balance, make it private
    pub async fn subscenario_2(&mut self) {
        let acc_addr = self.create_new_account().await;

        let resp = self.send_public_deposit(acc_addr, 100).await.unwrap();
        info!("Response for public deposit is {resp:?}");

        info!("Awaiting new blocks");
        tokio::time::sleep(std::time::Duration::from_secs(BLOCK_GEN_DELAY_SECS)).await;

        {
            let acc_map_read_guard = self.storage.read().await;

            let acc = acc_map_read_guard.acc_map.get(&acc_addr).unwrap();
            acc.log();

            info!("New acconut public balance is {:?}", acc.balance);
        };

        let (resp, new_utxo_hashes) = self
            .send_shielded_send_tx(acc_addr, 100, vec![(100, acc_addr)])
            .await
            .unwrap();
        info!("Response for send shielded is {resp:?}");

        let new_utxo_hash = new_utxo_hashes[0].1;

        info!("Awaiting new blocks");
        tokio::time::sleep(std::time::Duration::from_secs(BLOCK_GEN_DELAY_SECS)).await;

        let new_utxo = {
            let mut write_guard = self.storage.write().await;

            let acc = write_guard.acc_map.get_mut(&acc_addr).unwrap();
            acc.log();

            acc.utxo_tree
                .get_item(new_utxo_hash)
                .unwrap()
                .unwrap()
                .clone()
        };
        new_utxo.log();

        info!("User received new utxo {new_utxo:?}");
    }

    ///Mint utxo, privately send it to another user
    pub async fn subscenario_3(&mut self) {
        let acc_addr = self.create_new_account().await;

        let (resp, new_utxo_hash, comm_gen_hash) =
            self.send_private_mint_tx(acc_addr, 100).await.unwrap();
        info!("Response for mint private is {resp:?}");

        info!("Awaiting new blocks");
        tokio::time::sleep(std::time::Duration::from_secs(BLOCK_GEN_DELAY_SECS)).await;

        let new_utxo = {
            let mut write_guard = self.storage.write().await;

            let acc = write_guard.acc_map.get_mut(&acc_addr).unwrap();
            acc.log();

            acc.utxo_tree
                .get_item(new_utxo_hash)
                .unwrap()
                .unwrap()
                .clone()
        };
        new_utxo.log();

        let acc_addr_rec = self.create_new_account().await;

        let (resp, new_utxo_hashes) = self
            .send_private_send_tx(new_utxo, comm_gen_hash, vec![(100, acc_addr_rec)])
            .await
            .unwrap();
        info!("Response for send deshielded is {resp:?}");

        let new_utxo_hash = new_utxo_hashes[0].1;

        info!("Awaiting new blocks");
        tokio::time::sleep(std::time::Duration::from_secs(BLOCK_GEN_DELAY_SECS)).await;

        let new_utxo = {
            let mut write_guard = self.storage.write().await;

            let acc = write_guard.acc_map.get_mut(&acc_addr_rec).unwrap();
            acc.log();

            acc.utxo_tree
                .get_item(new_utxo_hash)
                .unwrap()
                .unwrap()
                .clone()
        };

        info!("User {acc_addr_rec:?} received new utxo {new_utxo:?}");
    }

    ///Deposit balance, shielded send it to another user
    pub async fn subscenario_4(&mut self) {
        let acc_addr = self.create_new_account().await;
        let acc_addr_rec = self.create_new_account().await;

        let resp = self.send_public_deposit(acc_addr, 100).await.unwrap();
        info!("Response for public deposit is {resp:?}");

        info!("Awaiting new blocks");
        tokio::time::sleep(std::time::Duration::from_secs(BLOCK_GEN_DELAY_SECS)).await;

        {
            let acc_map_read_guard = self.storage.read().await;
            let acc = acc_map_read_guard.acc_map.get(&acc_addr).unwrap();
            acc.log();

            info!("New acconut public balance is {:?}", acc.balance);
        }

        let (resp, new_utxo_hashes) = self
            .send_shielded_send_tx(acc_addr, 100, vec![(100, acc_addr_rec)])
            .await
            .unwrap();
        info!("Response for send shielded is {resp:?}");

        let new_utxo_hash = new_utxo_hashes[0].1;

        info!("Awaiting new blocks");
        tokio::time::sleep(std::time::Duration::from_secs(BLOCK_GEN_DELAY_SECS)).await;

        let new_utxo = {
            let mut write_guard = self.storage.write().await;

            let acc = write_guard.acc_map.get_mut(&acc_addr_rec).unwrap();
            acc.log();

            acc.utxo_tree
                .get_item(new_utxo_hash)
                .unwrap()
                .unwrap()
                .clone()
        };

        info!("User {acc_addr_rec:?} received new utxo {new_utxo:?}");
    }

    ///Mint utxo, deshielded send it to another user
    pub async fn subscenario_5(&mut self) {
        let acc_addr = self.create_new_account().await;
        let acc_addr_rec = self.create_new_account().await;

        let (resp, new_utxo_hash, comm_gen_hash) =
            self.send_private_mint_tx(acc_addr, 100).await.unwrap();
        info!("Response for mint private is {resp:?}");

        info!("Awaiting new blocks");
        tokio::time::sleep(std::time::Duration::from_secs(BLOCK_GEN_DELAY_SECS)).await;

        let new_utxo = {
            let mut write_guard = self.storage.write().await;

            let acc = write_guard.acc_map.get_mut(&acc_addr).unwrap();
            acc.log();

            acc.utxo_tree
                .get_item(new_utxo_hash)
                .unwrap()
                .unwrap()
                .clone()
        };
        new_utxo.log();

        let resp = self
            .send_deshielded_send_tx(new_utxo, comm_gen_hash, vec![(100, acc_addr_rec)])
            .await
            .unwrap();
        info!("Response for send deshielded is {resp:?}");

        info!("Awaiting new blocks");
        tokio::time::sleep(std::time::Duration::from_secs(BLOCK_GEN_DELAY_SECS)).await;

        {
            let read_guard = self.storage.read().await;
            let acc_rec = read_guard.acc_map.get(&acc_addr_rec).unwrap();
            acc_rec.log();

            info!("New account public balance is {:?}", acc_rec.balance);
        }
    }
}

use std::sync::{atomic::AtomicU64, Arc};

use common::{
    execution_input::PublicNativeTokenSend, transaction::Transaction, ExecutionFailureKind,
};

use accounts::account_core::{address::AccountAddress, Account};
use anyhow::Result;
use chain_storage::NodeChainStore;
use common::transaction::TransactionBody;
use config::NodeConfig;
use log::info;
use sc_core::proofs_circuits::{generate_commitments, pedersen_commitment_vec};
use sequencer_client::{json::SendTxResponse, SequencerClient};
use serde::{Deserialize, Serialize};
use storage::sc_db_utils::DataBlobChangeVariant;
use tokio::sync::RwLock;
use utxo::utxo_core::UTXO;
use zkvm::gas_calculator::GasCalculator;

pub const BLOCK_GEN_DELAY_SECS: u64 = 20;

pub mod chain_storage;
pub mod config;
pub mod sequencer_client;

pub fn vec_u8_to_vec_u64(bytes: Vec<u8>) -> Vec<u64> {
    // Pad with zeros to make sure it's a multiple of 8
    let mut padded = bytes.clone();
    while !padded.len().is_multiple_of(8) {
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
    pub sequencer_client: Arc<SequencerClient>,
    pub gas_calculator: GasCalculator,
}

impl NodeCore {
    pub async fn start_from_config_update_chain(config: NodeConfig) -> Result<Self> {
        let client = Arc::new(SequencerClient::new(config.clone())?);

        let genesis_id = client.get_genesis_id().await?;
        info!("Genesis id is {genesis_id:?}");

        let genesis_block = client.get_block(genesis_id.genesis_id).await?.block;

        let (mut storage, chain_height) = NodeChainStore::new(config.clone(), genesis_block)?;
        for acc in config.clone().initial_accounts {
            storage.acc_map.insert(acc.address, acc);
        }

        let wrapped_storage = Arc::new(RwLock::new(storage));
        let chain_height_wrapped = Arc::new(AtomicU64::new(chain_height));

        Ok(Self {
            storage: wrapped_storage,
            curr_height: chain_height_wrapped,
            node_config: config.clone(),
            sequencer_client: client.clone(),
            gas_calculator: GasCalculator::from(config.gas_config),
        })
    }

    pub async fn get_roots(&self) -> [[u8; 32]; 2] {
        let storage = self.storage.read().await;
        [
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

    pub async fn send_public_native_token_transfer(
        &self,
        from: AccountAddress,
        nonce: u64,
        to: AccountAddress,
        balance_to_move: u64,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let tx_roots = self.get_roots().await;

        let public_context = {
            let read_guard = self.storage.read().await;

            read_guard.produce_context(from)
        };

        let (tweak, secret_r, commitment) = pedersen_commitment_vec(
            //Will not panic, as public context is serializable
            public_context.produce_u64_list_from_context().unwrap(),
        );

        let sc_addr = hex::encode([0; 32]);

        //Native contract does not change its state
        let state_changes: Vec<DataBlobChangeVariant> = vec![];
        let new_len = 0;
        let state_changes = (serde_json::to_value(state_changes).unwrap(), new_len);

        let tx: TransactionBody =
            sc_core::transaction_payloads_tools::create_public_transaction_payload(
                serde_json::to_vec(&PublicNativeTokenSend {
                    from,
                    nonce,
                    to,
                    balance_to_move,
                })
                .unwrap(),
                commitment,
                tweak,
                secret_r,
                sc_addr,
                state_changes,
            );
        tx.log();

        {
            let read_guard = self.storage.read().await;

            let account = read_guard.acc_map.get(&from);

            if let Some(account) = account {
                let key_to_sign_transaction = account.key_holder.get_pub_account_signing_key();

                let signed_transaction = Transaction::new(tx, key_to_sign_transaction);

                Ok(self
                    .sequencer_client
                    .send_tx(signed_transaction, tx_roots)
                    .await?)
            } else {
                Err(ExecutionFailureKind::AmountMismatchError)
            }
        }
    }
}

pub fn generate_commitments_helper(input_utxos: &[UTXO]) -> Vec<[u8; 32]> {
    generate_commitments(input_utxos)
        .into_iter()
        .map(|comm_raw| comm_raw.try_into().unwrap())
        .collect()
}

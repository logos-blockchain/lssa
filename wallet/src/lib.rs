use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use chain_storage::WalletChainStore;
use common::{
    error::ExecutionFailureKind,
    sequencer_client::{SequencerClient, json::SendTxResponse},
    transaction::{EncodedTransaction, NSSATransaction},
};
use config::WalletConfig;
use key_protocol::key_management::key_tree::chain_index::ChainIndex;
use log::info;
use nssa::{Account, AccountId, PrivacyPreservingTransaction, program::Program};
use nssa_core::{Commitment, MembershipProof, SharedSecretKey, program::InstructionData};
pub use privacy_preserving_tx::PrivacyPreservingAccount;
use tokio::io::AsyncWriteExt;

use crate::{
    config::PersistentStorage,
    helperfunctions::{
        fetch_persistent_storage, get_home, produce_data_for_storage, produce_random_nonces,
    },
    poller::TxPoller,
};

pub const HOME_DIR_ENV_VAR: &str = "NSSA_WALLET_HOME_DIR";

pub mod chain_storage;
pub mod cli;
pub mod config;
pub mod helperfunctions;
pub mod poller;
mod privacy_preserving_tx;
pub mod program_facades;

pub struct WalletCore {
    pub storage: WalletChainStore,
    pub poller: TxPoller,
    pub sequencer_client: Arc<SequencerClient>,
    pub last_synced_block: u64,
}

impl WalletCore {
    pub async fn start_from_config_update_chain(config: WalletConfig) -> Result<Self> {
        let client = Arc::new(SequencerClient::new(config.sequencer_addr.clone())?);
        let tx_poller = TxPoller::new(config.clone(), client.clone());

        let PersistentStorage {
            accounts: persistent_accounts,
            last_synced_block,
        } = fetch_persistent_storage().await?;

        let storage = WalletChainStore::new(config, persistent_accounts)?;

        Ok(Self {
            storage,
            poller: tx_poller,
            sequencer_client: client.clone(),
            last_synced_block,
        })
    }

    pub async fn start_from_config_new_storage(
        config: WalletConfig,
        password: String,
    ) -> Result<Self> {
        let client = Arc::new(SequencerClient::new(config.sequencer_addr.clone())?);
        let tx_poller = TxPoller::new(config.clone(), client.clone());

        let storage = WalletChainStore::new_storage(config, password)?;

        Ok(Self {
            storage,
            poller: tx_poller,
            sequencer_client: client.clone(),
            last_synced_block: 0,
        })
    }

    /// Store persistent data at home
    pub async fn store_persistent_data(&self) -> Result<PathBuf> {
        let home = get_home()?;
        let storage_path = home.join("storage.json");

        let data = produce_data_for_storage(&self.storage.user_data, self.last_synced_block);
        let storage = serde_json::to_vec_pretty(&data)?;

        let mut storage_file = tokio::fs::File::create(storage_path.as_path()).await?;
        storage_file.write_all(&storage).await?;

        info!("Stored data at {storage_path:#?}");

        Ok(storage_path)
    }

    /// Store persistent data at home
    pub async fn store_config_changes(&self) -> Result<PathBuf> {
        let home = get_home()?;
        let config_path = home.join("wallet_config.json");
        let config = serde_json::to_vec_pretty(&self.storage.wallet_config)?;

        let mut config_file = tokio::fs::File::create(config_path.as_path()).await?;
        config_file.write_all(&config).await?;

        info!("Stored data at {config_path:#?}");

        Ok(config_path)
    }

    pub fn create_new_account_public(&mut self, chain_index: ChainIndex) -> AccountId {
        self.storage
            .user_data
            .generate_new_public_transaction_private_key(chain_index)
    }

    pub fn create_new_account_private(&mut self, chain_index: ChainIndex) -> AccountId {
        self.storage
            .user_data
            .generate_new_privacy_preserving_transaction_key_chain(chain_index)
    }

    /// Get account balance
    pub async fn get_account_balance(&self, acc: AccountId) -> Result<u128> {
        Ok(self
            .sequencer_client
            .get_account_balance(acc.to_string())
            .await?
            .balance)
    }

    /// Get accounts nonces
    pub async fn get_accounts_nonces(&self, accs: Vec<AccountId>) -> Result<Vec<u128>> {
        Ok(self
            .sequencer_client
            .get_accounts_nonces(accs.into_iter().map(|acc| acc.to_string()).collect())
            .await?
            .nonces)
    }

    /// Get account
    pub async fn get_account_public(&self, account_id: AccountId) -> Result<Account> {
        let response = self
            .sequencer_client
            .get_account(account_id.to_string())
            .await?;
        Ok(response.account)
    }

    pub fn get_account_public_signing_key(
        &self,
        account_id: &AccountId,
    ) -> Option<&nssa::PrivateKey> {
        self.storage
            .user_data
            .get_pub_account_signing_key(account_id)
    }

    pub fn get_account_private(&self, account_id: &AccountId) -> Option<Account> {
        self.storage
            .user_data
            .get_private_account(account_id)
            .map(|value| value.1.clone())
    }

    pub fn get_private_account_commitment(&self, account_id: &AccountId) -> Option<Commitment> {
        let (keys, account) = self.storage.user_data.get_private_account(account_id)?;
        Some(Commitment::new(&keys.nullifer_public_key, account))
    }

    /// Poll transactions
    pub async fn poll_native_token_transfer(&self, hash: String) -> Result<NSSATransaction> {
        let transaction_encoded = self.poller.poll_tx(hash).await?;
        let tx_base64_decode = BASE64.decode(transaction_encoded)?;
        let pub_tx = borsh::from_slice::<EncodedTransaction>(&tx_base64_decode).unwrap();

        Ok(NSSATransaction::try_from(&pub_tx)?)
    }

    pub async fn check_private_account_initialized(
        &self,
        account_id: &AccountId,
    ) -> Result<Option<MembershipProof>> {
        if let Some(acc_comm) = self.get_private_account_commitment(account_id) {
            self.sequencer_client
                .get_proof_for_commitment(acc_comm)
                .await
                .map_err(anyhow::Error::from)
        } else {
            Ok(None)
        }
    }

    pub fn decode_insert_privacy_preserving_transaction_results(
        &mut self,
        tx: nssa::privacy_preserving_transaction::PrivacyPreservingTransaction,
        acc_decode_data: &[(nssa_core::SharedSecretKey, AccountId)],
    ) -> Result<()> {
        for (output_index, (secret, acc_account_id)) in acc_decode_data.iter().enumerate() {
            let acc_ead = tx.message.encrypted_private_post_states[output_index].clone();
            let acc_comm = tx.message.new_commitments[output_index].clone();

            let res_acc = nssa_core::EncryptionScheme::decrypt(
                &acc_ead.ciphertext,
                secret,
                &acc_comm,
                output_index as u32,
            )
            .unwrap();

            println!("Received new acc {res_acc:#?}");

            self.storage
                .insert_private_account_data(*acc_account_id, res_acc);
        }

        println!("Transaction data is {:?}", tx.message);

        Ok(())
    }

    pub async fn send_privacy_preserving_tx(
        &self,
        accounts: Vec<PrivacyPreservingAccount>,
        instruction_data: &InstructionData,
        program: &Program,
    ) -> Result<(SendTxResponse, Vec<SharedSecretKey>), ExecutionFailureKind> {
        self.send_privacy_preserving_tx_with_pre_check(accounts, instruction_data, program, |_| {
            Ok(())
        })
        .await
    }

    pub async fn send_privacy_preserving_tx_with_pre_check(
        &self,
        accounts: Vec<PrivacyPreservingAccount>,
        instruction_data: &InstructionData,
        program: &Program,
        tx_pre_check: impl FnOnce(&[&Account]) -> Result<(), ExecutionFailureKind>,
    ) -> Result<(SendTxResponse, Vec<SharedSecretKey>), ExecutionFailureKind> {
        let acc_manager = privacy_preserving_tx::AccountManager::new(self, accounts).await?;

        let pre_states = acc_manager.pre_states();
        tx_pre_check(
            &pre_states
                .iter()
                .map(|pre| &pre.account)
                .collect::<Vec<_>>(),
        )?;

        let private_account_keys = acc_manager.private_account_keys();
        let (output, proof) = nssa::privacy_preserving_transaction::circuit::execute_and_prove(
            &pre_states,
            instruction_data,
            acc_manager.visibility_mask(),
            &produce_random_nonces(private_account_keys.len()),
            &private_account_keys
                .iter()
                .map(|keys| (keys.npk.clone(), keys.ssk.clone()))
                .collect::<Vec<_>>(),
            &acc_manager.private_account_auth(),
            program,
        )
        .unwrap();

        let message =
            nssa::privacy_preserving_transaction::message::Message::try_from_circuit_output(
                acc_manager.public_account_ids(),
                Vec::from_iter(acc_manager.public_account_nonces()),
                private_account_keys
                    .iter()
                    .map(|keys| (keys.npk.clone(), keys.ipk.clone(), keys.epk.clone()))
                    .collect(),
                output,
            )
            .unwrap();

        let witness_set =
            nssa::privacy_preserving_transaction::witness_set::WitnessSet::for_message(
                &message,
                proof,
                &acc_manager.witness_signing_keys(),
            );
        let tx = PrivacyPreservingTransaction::new(message, witness_set);

        let shared_secrets = private_account_keys
            .into_iter()
            .map(|keys| keys.ssk)
            .collect();

        Ok((
            self.sequencer_client.send_tx_private(tx).await?,
            shared_secrets,
        ))
    }
}

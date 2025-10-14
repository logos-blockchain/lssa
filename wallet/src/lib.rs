use std::{fs::File, io::Write, path::PathBuf, sync::Arc};

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use common::{
    sequencer_client::SequencerClient,
    transaction::{EncodedTransaction, NSSATransaction},
};

use anyhow::Result;
use chain_storage::WalletChainStore;
use config::WalletConfig;
use log::info;
use nssa::{Account, Address};

use clap::{Parser, Subcommand};
use nssa_core::Commitment;

use crate::cli::{
    WalletSubcommand, chain::ChainSubcommand,
    native_token_transfer_program::NativeTokenTransferProgramSubcommand,
    pinata_program::PinataProgramSubcommand,
};
use crate::{
    cli::token_program::TokenProgramSubcommand,
    helperfunctions::{
        fetch_config, fetch_persistent_accounts, get_home, produce_data_for_storage,
    },
    poller::TxPoller,
};

pub const HOME_DIR_ENV_VAR: &str = "NSSA_WALLET_HOME_DIR";

pub mod chain_storage;
pub mod cli;
pub mod config;
pub mod helperfunctions;
pub mod pinata_interactions;
pub mod poller;
pub mod token_program_interactions;
pub mod token_transfers;

pub struct WalletCore {
    pub storage: WalletChainStore,
    pub poller: TxPoller,
    pub sequencer_client: Arc<SequencerClient>,
}

impl WalletCore {
    pub fn start_from_config_update_chain(config: WalletConfig) -> Result<Self> {
        let client = Arc::new(SequencerClient::new(config.sequencer_addr.clone())?);
        let tx_poller = TxPoller::new(config.clone(), client.clone());

        let mut storage = WalletChainStore::new(config)?;

        let persistent_accounts = fetch_persistent_accounts()?;
        for pers_acc_data in persistent_accounts {
            storage.insert_account_data(pers_acc_data);
        }

        Ok(Self {
            storage,
            poller: tx_poller,
            sequencer_client: client.clone(),
        })
    }

    ///Store persistent accounts at home
    pub fn store_persistent_accounts(&self) -> Result<PathBuf> {
        let home = get_home()?;
        let accs_path = home.join("curr_accounts.json");

        let data = produce_data_for_storage(&self.storage.user_data);
        let accs = serde_json::to_vec_pretty(&data)?;

        let mut accs_file = File::create(accs_path.as_path())?;
        accs_file.write_all(&accs)?;

        info!("Stored accounts data at {accs_path:#?}");

        Ok(accs_path)
    }

    pub fn create_new_account_public(&mut self) -> Address {
        self.storage
            .user_data
            .generate_new_public_transaction_private_key()
    }

    pub fn create_new_account_private(&mut self) -> Address {
        self.storage
            .user_data
            .generate_new_privacy_preserving_transaction_key_chain()
    }

    ///Get account balance
    pub async fn get_account_balance(&self, acc: Address) -> Result<u128> {
        Ok(self
            .sequencer_client
            .get_account_balance(acc.to_string())
            .await?
            .balance)
    }

    ///Get accounts nonces
    pub async fn get_accounts_nonces(&self, accs: Vec<Address>) -> Result<Vec<u128>> {
        Ok(self
            .sequencer_client
            .get_accounts_nonces(accs.into_iter().map(|acc| acc.to_string()).collect())
            .await?
            .nonces)
    }

    ///Get account
    pub async fn get_account_public(&self, addr: Address) -> Result<Account> {
        let response = self.sequencer_client.get_account(addr.to_string()).await?;
        Ok(response.account)
    }

    pub fn get_account_private(&self, addr: &Address) -> Option<Account> {
        self.storage
            .user_data
            .user_private_accounts
            .get(addr)
            .map(|value| value.1.clone())
    }

    pub fn get_private_account_commitment(&self, addr: &Address) -> Option<Commitment> {
        let (keys, account) = self.storage.user_data.user_private_accounts.get(addr)?;
        Some(Commitment::new(&keys.nullifer_public_key, account))
    }

    ///Poll transactions
    pub async fn poll_native_token_transfer(&self, hash: String) -> Result<NSSATransaction> {
        let transaction_encoded = self.poller.poll_tx(hash).await?;
        let tx_base64_decode = BASE64.decode(transaction_encoded)?;
        let pub_tx = borsh::from_slice::<EncodedTransaction>(&tx_base64_decode).unwrap();

        Ok(NSSATransaction::try_from(&pub_tx)?)
    }

    pub async fn check_private_account_initialized(&self, addr: &Address) -> bool {
        if let Some(acc_comm) = self.get_private_account_commitment(addr) {
            matches!(
                self.sequencer_client
                    .get_proof_for_commitment(acc_comm)
                    .await,
                Ok(Some(_))
            )
        } else {
            false
        }
    }

    pub fn decode_insert_privacy_preserving_transaction_results(
        &mut self,
        tx: nssa::privacy_preserving_transaction::PrivacyPreservingTransaction,
        acc_decode_data: &[(nssa_core::SharedSecretKey, Address)],
    ) -> Result<()> {
        for (output_index, (secret, acc_address)) in acc_decode_data.iter().enumerate() {
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
                .insert_private_account_data(*acc_address, res_acc);
        }

        println!("Transaction data is {:?}", tx.message);

        Ok(())
    }
}

///Represents CLI command for a wallet
#[derive(Subcommand, Debug, Clone)]
#[clap(about)]
pub enum Command {
    ///Transfer command
    #[command(subcommand)]
    Transfer(NativeTokenTransferProgramSubcommand),
    ///Chain command
    #[command(subcommand)]
    Chain(ChainSubcommand),
    ///Pinata command
    #[command(subcommand)]
    PinataProgram(PinataProgramSubcommand),
    ///Token command
    #[command(subcommand)]
    TokenProgram(TokenProgramSubcommand),
}

///To execute commands, env var NSSA_WALLET_HOME_DIR must be set into directory with config
#[derive(Parser, Debug)]
#[clap(version, about)]
pub struct Args {
    /// Wallet command
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Clone)]
pub enum SubcommandReturnValue {
    PrivacyPreservingTransfer { tx_hash: String },
    RegisterAccount { addr: nssa::Address },
    Account(nssa::Account),
    Empty,
}

pub async fn execute_subcommand(command: Command) -> Result<SubcommandReturnValue> {
    let wallet_config = fetch_config()?;
    let mut wallet_core = WalletCore::start_from_config_update_chain(wallet_config)?;

    let subcommand_ret = match command {
        Command::Transfer(transfer_subcommand) => {
            transfer_subcommand
                .handle_subcommand(&mut wallet_core)
                .await?
        }
        Command::Chain(chain_subcommand) => {
            chain_subcommand.handle_subcommand(&mut wallet_core).await?
        }
        Command::PinataProgram(pinata_subcommand) => {
            pinata_subcommand
                .handle_subcommand(&mut wallet_core)
                .await?
        }
        Command::TokenProgram(token_subcommand) => {
            token_subcommand.handle_subcommand(&mut wallet_core).await?
        }
    };

    Ok(subcommand_ret)
}

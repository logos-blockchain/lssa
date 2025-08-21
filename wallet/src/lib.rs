use std::{fs::File, io::Write, path::PathBuf, sync::Arc};

use base64::Engine;
use common::{
    sequencer_client::{json::SendTxResponse, SequencerClient},
    ExecutionFailureKind,
};

use anyhow::Result;
use chain_storage::WalletChainStore;
use config::WalletConfig;
use log::info;
use nssa::Address;

use clap::{Parser, Subcommand};

use crate::{
    helperfunctions::{
        fetch_config, fetch_persistent_accounts, get_home, produce_account_addr_from_hex,
        produce_data_for_storage,
    },
    poller::TxPoller,
};

pub const HOME_DIR_ENV_VAR: &str = "NSSA_WALLET_HOME_DIR";
pub const BLOCK_GEN_DELAY_SECS: u64 = 20;

pub mod chain_storage;
pub mod config;
pub mod helperfunctions;
pub mod poller;

pub struct WalletCore {
    pub storage: WalletChainStore,
    pub poller: TxPoller,
    pub sequencer_client: Arc<SequencerClient>,
}

impl WalletCore {
    pub async fn start_from_config_update_chain(config: WalletConfig) -> Result<Self> {
        let client = Arc::new(SequencerClient::new(config.sequencer_addr.clone())?);
        let tx_poller = TxPoller {
            polling_delay_millis: config.seq_poll_timeout_millis,
            polling_max_blocks_to_query: config.seq_poll_max_blocks,
            polling_max_error_attempts: config.seq_poll_max_retries,
            polling_error_delay_millis: config.seq_poll_retry_delay_millis,
            client: client.clone(),
        };

        let mut storage = WalletChainStore::new(config)?;

        //Updating user data with stored accounts
        //We do not store/update any key data connected to private executions
        //ToDo: Add this into persistent data
        let persistent_accounts = fetch_persistent_accounts()?;
        for acc in persistent_accounts {
            storage
                .user_data
                .update_account_balance(acc.address, acc.account.balance);
        }

        Ok(Self {
            storage,
            poller: tx_poller,
            sequencer_client: client.clone(),
        })
    }

    pub fn store_persistent_accounts(&self) -> Result<PathBuf> {
        let home = get_home()?;
        let accs_path = home.join("curr_accounts.json");
        let accs = serde_json::to_vec(&produce_data_for_storage(&self.storage.user_data))?;

        let mut accs_file = File::create(accs_path.as_path())?;
        accs_file.write_all(&accs)?;

        info!("Stored accounts data at {accs_path:#?}");

        Ok(accs_path)
    }

    pub fn create_new_account(&mut self) -> Address {
        self.storage.user_data.generate_new_account()
    }

    pub async fn send_public_native_token_transfer(
        &self,
        from: Address,
        nonce: u128,
        to: Address,
        balance_to_move: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let account = self.storage.user_data.get_account(&from);

        if let Some(account) = account {
            if account.balance >= balance_to_move {
                let addresses = vec![from, to];
                let nonces = vec![nonce];
                let program_id = nssa::program::Program::authenticated_transfer_program().id();
                let message = nssa::public_transaction::Message::try_new(
                    program_id,
                    addresses,
                    nonces,
                    balance_to_move,
                )
                .unwrap();

                let signing_key = self.storage.user_data.get_account_signing_key(&from);

                if let Some(signing_key) = signing_key {
                    let witness_set =
                        nssa::public_transaction::WitnessSet::for_message(&message, &[signing_key]);

                    let tx = nssa::PublicTransaction::new(message, witness_set);

                    Ok(self.sequencer_client.send_tx(tx).await?)
                } else {
                    Err(ExecutionFailureKind::KeyNotFoundError)
                }
            } else {
                Err(ExecutionFailureKind::InsufficientFundsError)
            }
        } else {
            Err(ExecutionFailureKind::AmountMismatchError)
        }
    }

    pub async fn poll_public_native_token_transfer(
        &self,
        hash: String,
    ) -> Result<nssa::PublicTransaction> {
        let transaction_encoded = self.poller.poll_tx(hash).await?;
        let tx_base64_decode =
            base64::engine::general_purpose::STANDARD.decode(transaction_encoded)?;
        let pub_tx = nssa::PublicTransaction::from_bytes(&tx_base64_decode)?;

        Ok(pub_tx)
    }

    pub fn execute_native_token_transfer(
        &mut self,
        from: Address,
        to: Address,
        balance_to_move: u128,
    ) {
        self.storage.user_data.increment_account_nonce(from);
        self.storage.user_data.increment_account_nonce(to);

        let from_bal = self.storage.user_data.get_account_balance(&from);
        let to_bal = self.storage.user_data.get_account_balance(&to);

        self.storage
            .user_data
            .update_account_balance(from, from_bal - balance_to_move);
        self.storage
            .user_data
            .update_account_balance(to, to_bal + balance_to_move);
    }
}

///Represents CLI command for a wallet
#[derive(Subcommand, Debug, Clone)]
#[clap(about)]
pub enum Command {
    ///Send native token transfer from `from` to `to` for `amount`
    SendNativeTokenTransfer {
        ///from - valid 32 byte hex string
        #[arg(long)]
        from: String,
        ///nonce - u128 integer
        #[arg(long)]
        nonce: u128,
        ///to - valid 32 byte hex string
        #[arg(long)]
        to: String,
        ///amount - amount of balance to move
        #[arg(long)]
        amount: u128,
    },
    ///Register new account
    RegisterAccount {},
    ///Fetch transaction by `hash`
    FetchTx {
        #[arg(short, long)]
        tx_hash: String,
    },
}

///To execute commands, env var NSSA_WALLET_HOME_DIR must be set into directory with config
#[derive(Parser, Debug)]
#[clap(version, about)]
pub struct Args {
    /// Wallet command
    #[command(subcommand)]
    pub command: Command,
}

pub async fn execute_subcommand(command: Command) -> Result<()> {
    let wallet_config = fetch_config()?;
    let mut wallet_core = WalletCore::start_from_config_update_chain(wallet_config).await?;

    match command {
        Command::SendNativeTokenTransfer {
            from,
            nonce,
            to,
            amount,
        } => {
            let from = produce_account_addr_from_hex(from)?;
            let to = produce_account_addr_from_hex(to)?;

            let res = wallet_core
                .send_public_native_token_transfer(from, nonce, to, amount)
                .await?;

            info!("Results of tx send is {res:#?}");

            let transfer_tx = wallet_core
                .poll_public_native_token_transfer(res.tx_hash)
                .await?;

            info!("Transaction data is {transfer_tx:#?}");

            wallet_core.execute_native_token_transfer(from, to, amount);
        }
        Command::RegisterAccount {} => {
            let addr = wallet_core.create_new_account();

            let key = wallet_core.storage.user_data.get_account_signing_key(&addr);

            info!("Generated new account with addr {addr:#?}");
            info!("With key {key:#?}");
        }
        Command::FetchTx { tx_hash } => {
            let tx_obj = wallet_core
                .sequencer_client
                .get_transaction_by_hash(tx_hash)
                .await?;

            info!("Transaction object {tx_obj:#?}");
        }
    }

    wallet_core.store_persistent_accounts()?;

    Ok(())
}

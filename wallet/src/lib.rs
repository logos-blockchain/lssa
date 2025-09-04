use std::{fs::File, io::Write, path::PathBuf, str::FromStr, sync::Arc};

use base64::Engine;
use common::{
    ExecutionFailureKind,
    sequencer_client::{SequencerClient, json::SendTxResponse},
};

use anyhow::Result;
use chain_storage::WalletChainStore;
use config::WalletConfig;
use log::info;
use nssa::Address;

use clap::{Parser, Subcommand};
use nssa_core::account::Account;

use crate::{
    helperfunctions::{
        fetch_config, fetch_persistent_accounts, get_home, produce_account_addr_from_hex,
        produce_data_for_storage,
    },
    poller::TxPoller,
};

pub const HOME_DIR_ENV_VAR: &str = "NSSA_WALLET_HOME_DIR";

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

    pub fn create_new_account(&mut self) -> Address {
        self.storage.user_data.generate_new_account()
    }

    pub fn search_for_initial_account(&self, acc_addr: Address) -> Option<Account> {
        for initial_acc in &self.storage.wallet_config.initial_accounts {
            if initial_acc.address == acc_addr {
                return Some(initial_acc.account.clone());
            }
        }
        None
    }

    pub async fn send_public_native_token_transfer(
        &self,
        from: Address,
        to: Address,
        balance_to_move: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let Ok(balance) = self.get_account_balance(from).await else {
            return Err(ExecutionFailureKind::SequencerError);
        };

        if balance >= balance_to_move {
            let Ok(nonces) = self.get_accounts_nonces(vec![from]).await else {
                return Err(ExecutionFailureKind::SequencerError);
            };

            let addresses = vec![from, to];
            let program_id = nssa::program::Program::authenticated_transfer_program().id();
            let message = nssa::public_transaction::Message::try_new(
                program_id,
                addresses,
                nonces,
                balance_to_move,
            )
            .unwrap();

            let signing_key = self.storage.user_data.get_account_signing_key(&from);

            let Some(signing_key) = signing_key else {
                return Err(ExecutionFailureKind::KeyNotFoundError);
            };

            let witness_set =
                nssa::public_transaction::WitnessSet::for_message(&message, &[signing_key]);

            let tx = nssa::PublicTransaction::new(message, witness_set);

            Ok(self.sequencer_client.send_tx(tx).await?)
        } else {
            Err(ExecutionFailureKind::InsufficientFundsError)
        }
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

    ///Poll transactions
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
    ///Get account `addr` balance
    GetAccountBalance {
        #[arg(short, long)]
        addr: String,
    },
    ///Get account `addr` nonce
    GetAccountNonce {
        #[arg(short, long)]
        addr: String,
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
    let mut wallet_core = WalletCore::start_from_config_update_chain(wallet_config)?;

    match command {
        Command::SendNativeTokenTransfer { from, to, amount } => {
            let from = produce_account_addr_from_hex(from)?;
            let to = produce_account_addr_from_hex(to)?;

            let res = wallet_core
                .send_public_native_token_transfer(from, to, amount)
                .await?;

            info!("Results of tx send is {res:#?}");

            let transfer_tx = wallet_core
                .poll_public_native_token_transfer(res.tx_hash)
                .await?;

            info!("Transaction data is {transfer_tx:?}");
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
        Command::GetAccountBalance { addr } => {
            let addr = Address::from_str(&addr)?;

            let balance = wallet_core.get_account_balance(addr).await?;
            info!("Accounts {addr:#?} balance is {balance}");
        }
        Command::GetAccountNonce { addr } => {
            let addr = Address::from_str(&addr)?;

            let nonce = wallet_core.get_accounts_nonces(vec![addr]).await?[0];
            info!("Accounts {addr:#?} nonce is {nonce}");
        }
    }

    wallet_core.store_persistent_accounts()?;

    Ok(())
}

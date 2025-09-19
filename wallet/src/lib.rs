use std::{fs::File, io::Write, path::PathBuf, str::FromStr, sync::Arc};

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

use crate::{
    helperfunctions::{
        HumanReadableAccount, fetch_config, fetch_persistent_accounts, get_home,
        produce_account_addr_from_hex, produce_data_for_storage,
    },
    poller::TxPoller,
};

pub const HOME_DIR_ENV_VAR: &str = "NSSA_WALLET_HOME_DIR";

pub mod chain_storage;
pub mod config;
pub mod helperfunctions;
pub mod poller;
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
    pub async fn get_account(&self, addr: Address) -> Result<Account> {
        let response = self.sequencer_client.get_account(addr.to_string()).await?;
        Ok(response.account)
    }

    ///Poll transactions
    pub async fn poll_native_token_transfer(&self, hash: String) -> Result<NSSATransaction> {
        let transaction_encoded = self.poller.poll_tx(hash).await?;
        let tx_base64_decode = BASE64.decode(transaction_encoded)?;
        let pub_tx = EncodedTransaction::from_bytes(tx_base64_decode);

        Ok(NSSATransaction::try_from(&pub_tx)?)
    }
}

///Represents CLI command for a wallet
#[derive(Subcommand, Debug, Clone)]
#[clap(about)]
pub enum Command {
    ///Send native token transfer from `from` to `to` for `amount`
    ///
    /// Public operation
    SendNativeTokenTransferPublic {
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
    ///Send native token transfer from `from` to `to` for `amount`
    ///
    /// Private operation
    SendNativeTokenTransferPrivate {
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
    ///Register new public account
    RegisterAccountPublic {},
    ///Register new private account
    RegisterAccountPrivate {},
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
    ///Get account at address `addr`
    GetAccount {
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
        Command::SendNativeTokenTransferPublic { from, to, amount } => {
            let from = produce_account_addr_from_hex(from)?;
            let to = produce_account_addr_from_hex(to)?;

            let res = wallet_core
                .send_public_native_token_transfer(from, to, amount)
                .await?;

            println!("Results of tx send is {res:#?}");

            let transfer_tx = wallet_core.poll_native_token_transfer(res.tx_hash).await?;

            println!("Transaction data is {transfer_tx:?}");
        }
        Command::SendNativeTokenTransferPrivate { from, to, amount } => {
            let from = produce_account_addr_from_hex(from)?;
            let to = produce_account_addr_from_hex(to)?;

            let (res, secret) = wallet_core
                .send_private_native_token_transfer(from, to, amount)
                .await?;

            println!("Results of tx send is {res:#?}");

            let transfer_tx = wallet_core.poll_native_token_transfer(res.tx_hash).await?;

            if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                let from_ebc = tx.message.encrypted_private_post_states[0].clone();
                let from_comm = tx.message.new_commitments[0].clone();

                let to_ebc = tx.message.encrypted_private_post_states[1].clone();
                let to_comm = tx.message.new_commitments[1].clone();

                let res_acc = nssa_core::EncryptionScheme::decrypt(
                    &from_ebc.ciphertext,
                    &secret,
                    &from_comm,
                    0,
                )
                .unwrap();

                let res_acc_to =
                    nssa_core::EncryptionScheme::decrypt(&to_ebc.ciphertext, &secret, &to_comm, 1)
                        .unwrap();

                println!("RES acc {res_acc:#?}");
                println!("RES acc to {res_acc_to:#?}");

                println!("Transaction data is {:?}", tx.message);
            }
        }
        Command::RegisterAccountPublic {} => {
            let addr = wallet_core.create_new_account_public();

            println!("Generated new account with addr {addr}");
        }
        Command::RegisterAccountPrivate {} => {
            let addr = wallet_core.create_new_account_private();

            let (key, account) = wallet_core
                .storage
                .user_data
                .get_private_account(&addr)
                .unwrap();

            println!("Generated new account with addr {addr:#?}");
            println!("With key {key:#?}");
            println!("With account {account:#?}");
        }
        Command::FetchTx { tx_hash } => {
            let tx_obj = wallet_core
                .sequencer_client
                .get_transaction_by_hash(tx_hash)
                .await?;

            println!("Transaction object {tx_obj:#?}");
        }
        Command::GetAccountBalance { addr } => {
            let addr = Address::from_str(&addr)?;

            let balance = wallet_core.get_account_balance(addr).await?;
            println!("Accounts {addr} balance is {balance}");
        }
        Command::GetAccountNonce { addr } => {
            let addr = Address::from_str(&addr)?;

            let nonce = wallet_core.get_accounts_nonces(vec![addr]).await?[0];
            println!("Accounts {addr} nonce is {nonce}");
        }
        Command::GetAccount { addr } => {
            let addr: Address = addr.parse()?;
            let account: HumanReadableAccount = wallet_core.get_account(addr).await?.into();
            println!("{}", serde_json::to_string(&account).unwrap());
        }
    }

    let path = wallet_core.store_persistent_accounts()?;

    println!("Stored persistent accounts at {path:#?}");

    Ok(())
}

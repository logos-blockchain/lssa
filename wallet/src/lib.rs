use std::sync::Arc;

use common::{
    execution_input::PublicNativeTokenSend,
    sequencer_client::{json::SendTxResponse, SequencerClient},
    transaction::Transaction,
    ExecutionFailureKind,
};

use accounts::account_core::{address::AccountAddress, Account};
use anyhow::Result;
use chain_storage::WalletChainStore;
use common::transaction::TransactionBody;
use config::WalletConfig;
use log::info;
use sc_core::proofs_circuits::pedersen_commitment_vec;
use tokio::sync::RwLock;

use clap::{Parser, Subcommand};

use crate::helperfunctions::{fetch_config, produce_account_addr_from_hex};

pub const HOME_DIR_ENV_VAR: &str = "NSSA_WALLET_HOME_DIR";
pub const BLOCK_GEN_DELAY_SECS: u64 = 20;

pub mod chain_storage;
pub mod config;
pub mod helperfunctions;

pub struct WalletCore {
    pub storage: Arc<RwLock<WalletChainStore>>,
    pub wallet_config: WalletConfig,
    pub sequencer_client: Arc<SequencerClient>,
}

impl WalletCore {
    pub async fn start_from_config_update_chain(config: WalletConfig) -> Result<Self> {
        let client = Arc::new(SequencerClient::new(config.sequencer_addr.clone())?);

        let mut storage = WalletChainStore::new(config.clone())?;
        for acc in config.clone().initial_accounts {
            storage.acc_map.insert(acc.address, acc);
        }

        let wrapped_storage = Arc::new(RwLock::new(storage));

        Ok(Self {
            storage: wrapped_storage,
            wallet_config: config.clone(),
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

    pub async fn send_public_native_token_transfer(
        &self,
        from: AccountAddress,
        nonce: u64,
        to: AccountAddress,
        balance_to_move: u64,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let public_context = {
            let read_guard = self.storage.read().await;

            read_guard.produce_context(from)
        };

        let (tweak, secret_r, commitment) = pedersen_commitment_vec(
            //Will not panic, as public context is serializable
            public_context.produce_u64_list_from_context().unwrap(),
        );

        let sc_addr = hex::encode([0; 32]);

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
            );
        tx.log();

        {
            let read_guard = self.storage.read().await;

            let account = read_guard.acc_map.get(&from);

            if let Some(account) = account {
                let key_to_sign_transaction = account.key_holder.get_pub_account_signing_key();

                let signed_transaction = Transaction::new(tx, key_to_sign_transaction);

                Ok(self.sequencer_client.send_tx(signed_transaction).await?)
            } else {
                Err(ExecutionFailureKind::AmountMismatchError)
            }
        }
    }
}

///Represents CLI command for a wallet
#[derive(Subcommand, Debug, Clone)]
#[clap(about)]
pub enum Command {
    SendNativeTokenTransfer {
        ///from - valid 32 byte hex string
        #[arg(long)]
        from: String,
        ///nonce - u64 integer
        #[arg(long)]
        nonce: u64,
        ///to - valid 32 byte hex string
        #[arg(long)]
        to: String,
        ///amount - amount of balance to move
        #[arg(long)]
        amount: u64,
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
    match command {
        Command::SendNativeTokenTransfer {
            from,
            nonce,
            to,
            amount,
        } => {
            let wallet_config = fetch_config()?;

            let from = produce_account_addr_from_hex(from)?;
            let to = produce_account_addr_from_hex(to)?;

            let wallet_core = WalletCore::start_from_config_update_chain(wallet_config).await?;

            let res = wallet_core
                .send_public_native_token_transfer(from, nonce, to, amount)
                .await?;

            info!("Results of tx send is {res:#?}");
        }
    }

    Ok(())
}

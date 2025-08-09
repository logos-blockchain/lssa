use std::sync::Arc;

use common::{
    sequencer_client::{json::SendTxResponse, SequencerClient},
    ExecutionFailureKind,
};

use accounts::account_core::{address::AccountAddress, Account};
use anyhow::Result;
use chain_storage::NodeChainStore;
use config::NodeConfig;
use log::info;
use tokio::sync::RwLock;

use clap::{Parser, Subcommand};

use crate::helperfunctions::{fetch_config, produce_account_addr_from_hex};

pub const HOME_DIR_ENV_VAR: &str = "NSSA_WALLET_HOME_DIR";
pub const BLOCK_GEN_DELAY_SECS: u64 = 20;

pub mod chain_storage;
pub mod config;
pub mod helperfunctions;
pub mod requests_structs;

pub struct NodeCore {
    pub storage: Arc<RwLock<NodeChainStore>>,
    pub node_config: NodeConfig,
    pub sequencer_client: Arc<SequencerClient>,
}

impl NodeCore {
    pub async fn start_from_config_update_chain(config: NodeConfig) -> Result<Self> {
        let client = Arc::new(SequencerClient::new(config.sequencer_addr.clone())?);

        let mut storage = NodeChainStore::new(config.clone())?;
        for acc in config.clone().initial_accounts {
            storage.acc_map.insert(acc.address, acc);
        }

        let wrapped_storage = Arc::new(RwLock::new(storage));

        Ok(Self {
            storage: wrapped_storage,
            node_config: config.clone(),
            sequencer_client: client.clone(),
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
        nonce: u128,
        to: AccountAddress,
        balance_to_move: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        {
            let read_guard = self.storage.read().await;

            let account = read_guard.acc_map.get(&from);

            if let Some(account) = account {
                let addresses = vec![nssa::Address::new(from), nssa::Address::new(to)];
                let nonces = vec![nonce];
                let program_id = nssa::program::AUTHENTICATED_TRANSFER_PROGRAM.id();
                let message = nssa::public_transaction::Message::new(
                    program_id,
                    addresses,
                    nonces,
                    balance_to_move,
                );

                let signing_key = account.key_holder.get_pub_account_signing_key();
                let witness_set =
                    nssa::public_transaction::WitnessSet::for_message(&message, &[signing_key]);

                let tx = nssa::PublicTransaction::new(message, witness_set);

                Ok(self.sequencer_client.send_tx(tx).await?)
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
        ///to - valid 32 byte hex string
        #[arg(long)]
        to: String,
        ///amount - amount of balance to move
        #[arg(long)]
        amount: u128,
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
        Command::SendNativeTokenTransfer { from, to, amount } => {
            let node_config = fetch_config()?;

            let from = produce_account_addr_from_hex(from)?;
            let to = produce_account_addr_from_hex(to)?;

            let wallet_core = NodeCore::start_from_config_update_chain(node_config).await?;

            //ToDo: Nonce management
            let res = wallet_core
                .send_public_native_token_transfer(from, 0, to, amount)
                .await?;

            info!("Results of tx send is {res:#?}");
        }
    }

    Ok(())
}

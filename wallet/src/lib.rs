use std::{fs::File, io::Write, path::PathBuf, sync::Arc};

use common::{
    sequencer_client::{json::SendTxResponse, SequencerClient},
    ExecutionFailureKind,
};

use accounts::account_core::Account;
use anyhow::Result;
use chain_storage::WalletChainStore;
use config::WalletConfig;
use log::info;
use nssa::Address;

use clap::{Parser, Subcommand};

use crate::helperfunctions::{
    fetch_config, fetch_persistent_accounts, get_home, produce_account_addr_from_hex,
};

pub const HOME_DIR_ENV_VAR: &str = "NSSA_WALLET_HOME_DIR";
pub const BLOCK_GEN_DELAY_SECS: u64 = 20;

pub mod chain_storage;
pub mod config;
pub mod helperfunctions;

pub struct WalletCore {
    pub storage: WalletChainStore,
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

        //Persistent accounts take precedence for initial accounts
        let persistent_accounts = fetch_persistent_accounts()?;

        info!("Fetched persistent accoounts {persistent_accounts:#?}");

        for acc in persistent_accounts {
            storage.acc_map.insert(acc.address, acc);
        }

        Ok(Self {
            storage,
            wallet_config: config.clone(),
            sequencer_client: client.clone(),
        })
    }

    pub async fn create_new_account(&mut self) -> Address {
        let account = Account::new();
        account.log();

        let addr = account.address;

        self.storage.acc_map.insert(account.address, account);

        addr
    }

    pub async fn send_public_native_token_transfer(
        &self,
        from: Address,
        nonce: u128,
        to: Address,
        balance_to_move: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let account = self.storage.acc_map.get(&from);

        if let Some(account) = account {
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

            let signing_key = account.key_holder.get_pub_account_signing_key();
            let witness_set =
                nssa::public_transaction::WitnessSet::for_message(&message, &[signing_key]);

            let tx = nssa::PublicTransaction::new(message, witness_set);

            Ok(self.sequencer_client.send_tx(tx).await?)
        } else {
            Err(ExecutionFailureKind::AmountMismatchError)
        }
    }

    ///Dumps all accounts from acc_map at `path`
    ///
    ///Currently storing everything in one file
    ///
    ///ToDo: extend storage
    pub fn store_present_accounts_at_path(&self, path: PathBuf) -> Result<PathBuf> {
        let dump_path = path.join("curr_accounts.json");

        let curr_accs: Vec<Account> = self.storage.acc_map.values().cloned().collect();
        let accs_serialized = serde_json::to_vec_pretty(&curr_accs)?;

        let mut acc_file = File::create(&dump_path).unwrap();
        acc_file.write_all(&accs_serialized).unwrap();

        Ok(dump_path)
    }

    ///Dumps all accounts from acc_map at `NSSA_WALLET_HOME_DIR`
    ///
    ///Currently storing everything in one file
    ///
    ///ToDo: extend storage
    pub fn store_present_accounts_at_home(&self) -> Result<PathBuf> {
        let home = get_home()?;
        self.store_present_accounts_at_path(home)
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
    ///Dump accounts at destination
    DumpAccountsOnDisc {
        ///Dump path for accounts
        #[arg(short, long)]
        dump_path: PathBuf,
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

            //ToDo: Insert transaction polling logic here

            let acc_storage_path = wallet_core.store_present_accounts_at_home()?;

            info!("Accounts stored at {acc_storage_path:#?}");
        }
        Command::DumpAccountsOnDisc { dump_path } => {
            let node_config = fetch_config()?;

            let wallet_core = WalletCore::start_from_config_update_chain(node_config).await?;

            wallet_core.store_present_accounts_at_path(dump_path.clone())?;

            info!("Accounts stored at path {dump_path:#?}");
        }
    }

    Ok(())
}

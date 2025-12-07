use anyhow::Result;
use clap::{Parser, Subcommand};
use nssa::program::Program;

use crate::{
    WalletCore,
    cli::{
        account::AccountSubcommand,
        chain::ChainSubcommand,
        config::ConfigSubcommand,
        programs::{
            native_token_transfer::AuthTransferSubcommand, pinata::PinataProgramAgnosticSubcommand,
            token::TokenProgramAgnosticSubcommand,
        },
    },
    helperfunctions::fetch_config,
};

pub mod account;
pub mod chain;
pub mod config;
pub mod programs;

pub(crate) trait WalletSubcommand {
    async fn handle_subcommand(self, wallet_core: &mut WalletCore)
    -> Result<SubcommandReturnValue>;
}

/// Represents CLI command for a wallet
#[derive(Subcommand, Debug, Clone)]
#[clap(about)]
pub enum Command {
    /// Authenticated transfer subcommand
    #[command(subcommand)]
    AuthTransfer(AuthTransferSubcommand),
    /// Generic chain info subcommand
    #[command(subcommand)]
    ChainInfo(ChainSubcommand),
    /// Account view and sync subcommand
    #[command(subcommand)]
    Account(AccountSubcommand),
    /// Pinata program interaction subcommand
    #[command(subcommand)]
    Pinata(PinataProgramAgnosticSubcommand),
    /// Token program interaction subcommand
    #[command(subcommand)]
    Token(TokenProgramAgnosticSubcommand),
    /// Check the wallet can connect to the node and builtin local programs
    /// match the remote versions
    CheckHealth {},
    /// Command to setup config, get and set config fields
    #[command(subcommand)]
    Config(ConfigSubcommand),
}

/// Represents overarching CLI command for a wallet with setup included
#[derive(Debug, Subcommand, Clone)]
#[clap(about)]
pub enum OverCommand {
    /// Represents CLI command for a wallet
    #[command(subcommand)]
    Command(Command),
    /// Setup of a storage. Initializes rots for public and private trees from `password`.
    Setup {
        #[arg(short, long)]
        password: String,
    },
}

/// To execute commands, env var NSSA_WALLET_HOME_DIR must be set into directory with config
///
/// All account adresses must be valid 32 byte base58 strings.
///
/// All account account_ids must be provided as {privacy_prefix}/{account_id},
/// where valid options for `privacy_prefix` is `Public` and `Private`
#[derive(Parser, Debug)]
#[clap(version, about)]
pub struct Args {
    /// Continious run flag
    #[arg(short, long)]
    pub continuous_run: bool,
    /// Wallet command
    #[command(subcommand)]
    pub command: Option<OverCommand>,
}

#[derive(Debug, Clone)]
pub enum SubcommandReturnValue {
    PrivacyPreservingTransfer { tx_hash: String },
    RegisterAccount { account_id: nssa::AccountId },
    Account(nssa::Account),
    Empty,
    SyncedToBlock(u64),
}

pub async fn execute_subcommand(command: Command) -> Result<SubcommandReturnValue> {
    let wallet_config = fetch_config().await?;
    let mut wallet_core = WalletCore::start_from_config_update_chain(wallet_config).await?;

    let subcommand_ret = match command {
        Command::AuthTransfer(transfer_subcommand) => {
            transfer_subcommand
                .handle_subcommand(&mut wallet_core)
                .await?
        }
        Command::ChainInfo(chain_subcommand) => {
            chain_subcommand.handle_subcommand(&mut wallet_core).await?
        }
        Command::Account(account_subcommand) => {
            account_subcommand
                .handle_subcommand(&mut wallet_core)
                .await?
        }
        Command::Pinata(pinata_subcommand) => {
            pinata_subcommand
                .handle_subcommand(&mut wallet_core)
                .await?
        }
        Command::CheckHealth {} => {
            let remote_program_ids = wallet_core
                .sequencer_client
                .get_program_ids()
                .await
                .expect("Error fetching program ids");
            let Some(authenticated_transfer_id) = remote_program_ids.get("authenticated_transfer")
            else {
                panic!("Missing authenticated transfer ID from remote");
            };
            if authenticated_transfer_id != &Program::authenticated_transfer_program().id() {
                panic!("Local ID for authenticated transfer program is different from remote");
            }
            let Some(token_id) = remote_program_ids.get("token") else {
                panic!("Missing token program ID from remote");
            };
            if token_id != &Program::token().id() {
                panic!("Local ID for token program is different from remote");
            }
            let Some(circuit_id) = remote_program_ids.get("privacy_preserving_circuit") else {
                panic!("Missing privacy preserving circuit ID from remote");
            };
            if circuit_id != &nssa::PRIVACY_PRESERVING_CIRCUIT_ID {
                panic!("Local ID for privacy preserving circuit is different from remote");
            }

            println!("✅All looks good!");

            SubcommandReturnValue::Empty
        }
        Command::Token(token_subcommand) => {
            token_subcommand.handle_subcommand(&mut wallet_core).await?
        }
        Command::Config(config_subcommand) => {
            config_subcommand
                .handle_subcommand(&mut wallet_core)
                .await?
        }
    };

    Ok(subcommand_ret)
}

pub async fn execute_continuous_run() -> Result<()> {
    let config = fetch_config().await?;
    let mut wallet_core = WalletCore::start_from_config_update_chain(config.clone()).await?;

    loop {
        let latest_block_num = wallet_core
            .sequencer_client
            .get_last_block()
            .await?
            .last_block;
        wallet_core.sync_to_block(latest_block_num).await?;

        tokio::time::sleep(std::time::Duration::from_millis(
            config.seq_poll_timeout_millis,
        ))
        .await;
    }
}

pub async fn execute_setup(password: String) -> Result<()> {
    let config = fetch_config().await?;
    let wallet_core = WalletCore::start_from_config_new_storage(config.clone(), password).await?;

    wallet_core.store_persistent_data().await?;

    Ok(())
}

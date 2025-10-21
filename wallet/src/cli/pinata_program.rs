use anyhow::Result;
use clap::Subcommand;
use common::transaction::NSSATransaction;
use log::info;

use crate::{SubcommandReturnValue, WalletCore, cli::WalletSubcommand};

///Represents generic CLI subcommand for a wallet working with pinata program
#[derive(Subcommand, Debug, Clone)]
pub enum PinataProgramSubcommand {
    ///Public execution
    #[command(subcommand)]
    Public(PinataProgramSubcommandPublic),
    ///Private execution
    #[command(subcommand)]
    Private(PinataProgramSubcommandPrivate),
}

///Represents generic public CLI subcommand for a wallet working with pinata program
#[derive(Subcommand, Debug, Clone)]
pub enum PinataProgramSubcommandPublic {
    // TODO: Testnet only. Refactor to prevent compilation on mainnet.
    // Claim piñata prize
    Claim {
        ///pinata_addr - valid 32 byte hex string
        #[arg(long)]
        pinata_addr: String,
        ///winner_addr - valid 32 byte hex string
        #[arg(long)]
        winner_addr: String,
        ///solution - solution to pinata challenge
        #[arg(long)]
        solution: u128,
    },
}

///Represents generic private CLI subcommand for a wallet working with pinata program
#[derive(Subcommand, Debug, Clone)]
pub enum PinataProgramSubcommandPrivate {
    // TODO: Testnet only. Refactor to prevent compilation on mainnet.
    // Claim piñata prize
    ClaimPrivateOwned {
        ///pinata_addr - valid 32 byte hex string
        #[arg(long)]
        pinata_addr: String,
        ///winner_addr - valid 32 byte hex string
        #[arg(long)]
        winner_addr: String,
        ///solution - solution to pinata challenge
        #[arg(long)]
        solution: u128,
    },
}

impl WalletSubcommand for PinataProgramSubcommandPublic {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            PinataProgramSubcommandPublic::Claim {
                pinata_addr,
                winner_addr,
                solution,
            } => {
                let res = wallet_core
                    .claim_pinata(
                        pinata_addr.parse().unwrap(),
                        winner_addr.parse().unwrap(),
                        solution,
                    )
                    .await?;
                info!("Results of tx send is {res:#?}");

                Ok(SubcommandReturnValue::Empty)
            }
        }
    }
}

impl WalletSubcommand for PinataProgramSubcommandPrivate {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            PinataProgramSubcommandPrivate::ClaimPrivateOwned {
                pinata_addr,
                winner_addr,
                solution,
            } => {
                let pinata_addr = pinata_addr.parse().unwrap();
                let winner_addr = winner_addr.parse().unwrap();

                let winner_initialization = wallet_core
                    .check_private_account_initialized(&winner_addr)
                    .await?;

                let (res, [secret_winner]) = if let Some(winner_proof) = winner_initialization {
                    wallet_core
                        .claim_pinata_private_owned_account_already_initialized(
                            pinata_addr,
                            winner_addr,
                            solution,
                            winner_proof,
                        )
                        .await?
                } else {
                    wallet_core
                        .claim_pinata_private_owned_account_not_initialized(
                            pinata_addr,
                            winner_addr,
                            solution,
                        )
                        .await?
                };

                info!("Results of tx send is {res:#?}");

                let tx_hash = res.tx_hash;
                let transfer_tx = wallet_core
                    .poll_native_token_transfer(tx_hash.clone())
                    .await?;

                if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                    let acc_decode_data = vec![(secret_winner, winner_addr)];

                    wallet_core.decode_insert_privacy_preserving_transaction_results(
                        tx,
                        &acc_decode_data,
                    )?;
                }

                let path = wallet_core.store_persistent_accounts().await?;

                println!("Stored persistent accounts at {path:#?}");

                Ok(SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash })
            }
        }
    }
}

impl WalletSubcommand for PinataProgramSubcommand {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            PinataProgramSubcommand::Private(private_subcommand) => {
                private_subcommand.handle_subcommand(wallet_core).await
            }
            PinataProgramSubcommand::Public(public_subcommand) => {
                public_subcommand.handle_subcommand(wallet_core).await
            }
        }
    }
}

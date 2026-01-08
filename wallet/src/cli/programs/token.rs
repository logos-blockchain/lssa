use anyhow::Result;
use clap::{Args, Subcommand};
use common::transaction::NSSATransaction;
use paste::paste;

use crate::{
    PrivacyPreservingAccount, WalletCore,
    cli::{
        SubcommandReturnValue, WalletSubcommand,
        programs::{ArgsReceiverMaybeUnowned, ArgsSenderOwned, ParsePrivacyPreservingAccount},
    },
    helperfunctions::{AccountPrivacyKind, parse_addr_with_privacy_prefix},
    maybe_unowned_account_name, owned_account_name,
    program_facades::{
        send_privacy_preserving_transaction_unified,
        token::{Token, TokenBurnArgs, TokenDefinitionArgs, TokenMintArgs, TokenTransferArgs},
    },
};

owned_account_name!(ArgsDefinitionOwned, definition_account_id);
owned_account_name!(ArgsSupplyOwned, supply_account_id);
owned_account_name!(ArgsHolderOwned, holder_account_id);
maybe_unowned_account_name!(ArgsHolderMaybeUnowned, holder);

/// Represents generic CLI subcommand for a wallet working with token program
#[derive(Subcommand, Debug, Clone)]
pub enum TokenProgramAgnosticSubcommand {
    /// Produce a new token
    New {
        #[command(flatten)]
        definition: ArgsDefinitionOwned,
        #[command(flatten)]
        supply: ArgsSupplyOwned,
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        total_supply: u128,
    },
    /// Send tokens from one account to another with variable privacy
    ///
    /// If receiver is private, then `to` and (`to_npk` , `to_ipk`) is a mutually exclusive
    /// patterns.
    ///
    /// First is used for owned accounts, second otherwise.
    Send {
        #[command(flatten)]
        from: ArgsSenderOwned,
        #[command(flatten)]
        to: ArgsReceiverMaybeUnowned,
        /// amount - amount of balance to move
        #[arg(long)]
        amount: u128,
    },
    /// Burn tokens on `holder`, modify `definition`.
    ///
    /// `holder` is owned
    ///
    /// Also if `definition` is private then it is owned, because
    /// we can not modify foreign accounts.
    Burn {
        #[command(flatten)]
        definition: ArgsDefinitionOwned,
        #[command(flatten)]
        holder: ArgsHolderOwned,
        /// amount - amount of balance to burn
        #[arg(long)]
        amount: u128,
    },
    /// Mint tokens on `holder`, modify `definition`.
    ///
    /// `definition` is owned
    ///
    /// If `holder` is private, then `holder` and (`holder_npk` , `holder_ipk`) is a mutually
    /// exclusive patterns.
    ///
    /// First is used for owned accounts, second otherwise.
    Mint {
        #[command(flatten)]
        definition: ArgsDefinitionOwned,
        #[command(flatten)]
        holder: ArgsHolderMaybeUnowned,
        /// amount - amount of balance to mint
        #[arg(long)]
        amount: u128,
    },
}

impl WalletSubcommand for TokenProgramAgnosticSubcommand {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            TokenProgramAgnosticSubcommand::New {
                definition,
                supply,
                name,
                total_supply,
            } => {
                let definition = definition.parse()?;
                let supply = supply.parse()?;

                if definition.is_private() || supply.is_private() {
                    let acc_vector = vec![definition, supply];

                    let name = name.as_bytes();
                    if name.len() > 6 {
                        // TODO: return error
                        panic!("Name length mismatch");
                    }
                    let mut name_bytes = [0; 6];
                    name_bytes[..name.len()].copy_from_slice(name);

                    let (res, acc_decode_data) = send_privacy_preserving_transaction_unified(
                        wallet_core,
                        acc_vector,
                        TokenDefinitionArgs {
                            name: name_bytes,
                            total_supply,
                        },
                    )
                    .await?;

                    println!("Results of tx send are {res:#?}");

                    let tx_hash = res.tx_hash;
                    let transfer_tx = wallet_core
                        .poll_native_token_transfer(tx_hash.clone())
                        .await?;

                    if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                        wallet_core.decode_insert_privacy_preserving_transaction_results(
                            tx,
                            &acc_decode_data,
                        )?;
                    }

                    let path = wallet_core.store_persistent_data().await?;

                    println!("Stored persistent accounts at {path:#?}");

                    Ok(SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash })
                } else {
                    let name = name.as_bytes();
                    if name.len() > 6 {
                        // TODO: return error
                        panic!();
                    }
                    let mut name_bytes = [0; 6];
                    name_bytes[..name.len()].copy_from_slice(name);
                    Token(wallet_core)
                        .send_new_definition(
                            definition
                                .account_id()
                                .expect("Public account can not be unowned"),
                            supply
                                .account_id()
                                .expect("Public account can not be unowned"),
                            name_bytes,
                            total_supply,
                        )
                        .await?;
                    Ok(SubcommandReturnValue::Empty)
                }
            }
            TokenProgramAgnosticSubcommand::Send { from, to, amount } => {
                let from = from.parse()?;
                let to = to.parse()?;

                if from.is_private() || to.is_private() {
                    let acc_vector = vec![from, to];

                    let (res, acc_decode_data) = send_privacy_preserving_transaction_unified(
                        wallet_core,
                        acc_vector,
                        TokenTransferArgs { amount },
                    )
                    .await?;

                    println!("Results of tx send are {res:#?}");

                    let tx_hash = res.tx_hash;
                    let transfer_tx = wallet_core
                        .poll_native_token_transfer(tx_hash.clone())
                        .await?;

                    if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                        wallet_core.decode_insert_privacy_preserving_transaction_results(
                            tx,
                            &acc_decode_data,
                        )?;
                    }

                    let path = wallet_core.store_persistent_data().await?;

                    println!("Stored persistent accounts at {path:#?}");

                    Ok(SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash })
                } else {
                    Token(wallet_core)
                        .send_transfer_transaction(
                            from.account_id()
                                .expect("Public account can not be unowned"),
                            to.account_id().expect("Public account can not be unowned"),
                            amount,
                        )
                        .await?;
                    Ok(SubcommandReturnValue::Empty)
                }
            }
            TokenProgramAgnosticSubcommand::Burn {
                definition,
                holder,
                amount,
            } => {
                let definition = definition.parse()?;
                let holder = holder.parse()?;

                if definition.is_private() || holder.is_private() {
                    let acc_vector = vec![definition, holder];

                    let (res, acc_decode_data) = send_privacy_preserving_transaction_unified(
                        wallet_core,
                        acc_vector,
                        TokenBurnArgs { amount },
                    )
                    .await?;

                    println!("Results of tx send are {res:#?}");

                    let tx_hash = res.tx_hash;
                    let transfer_tx = wallet_core
                        .poll_native_token_transfer(tx_hash.clone())
                        .await?;

                    if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                        wallet_core.decode_insert_privacy_preserving_transaction_results(
                            tx,
                            &acc_decode_data,
                        )?;
                    }

                    let path = wallet_core.store_persistent_data().await?;

                    println!("Stored persistent accounts at {path:#?}");

                    Ok(SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash })
                } else {
                    Token(wallet_core)
                        .send_burn_transaction(
                            definition
                                .account_id()
                                .expect("Public account can not be unowned"),
                            holder
                                .account_id()
                                .expect("Public account can not be unowned"),
                            amount,
                        )
                        .await?;
                    Ok(SubcommandReturnValue::Empty)
                }
            }
            TokenProgramAgnosticSubcommand::Mint {
                definition,
                holder,
                amount,
            } => {
                let definition = definition.parse()?;
                let holder = holder.parse()?;

                if definition.is_private() || holder.is_private() {
                    let acc_vector = vec![definition, holder];

                    let (res, acc_decode_data) = send_privacy_preserving_transaction_unified(
                        wallet_core,
                        acc_vector,
                        TokenMintArgs { amount },
                    )
                    .await?;

                    println!("Results of tx send are {res:#?}");

                    let tx_hash = res.tx_hash;
                    let transfer_tx = wallet_core
                        .poll_native_token_transfer(tx_hash.clone())
                        .await?;

                    if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                        wallet_core.decode_insert_privacy_preserving_transaction_results(
                            tx,
                            &acc_decode_data,
                        )?;
                    }

                    let path = wallet_core.store_persistent_data().await?;

                    println!("Stored persistent accounts at {path:#?}");

                    Ok(SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash })
                } else {
                    Token(wallet_core)
                        .send_mint_transaction(
                            definition
                                .account_id()
                                .expect("Public account can not be unowned"),
                            holder
                                .account_id()
                                .expect("Public account can not be unowned"),
                            amount,
                        )
                        .await?;
                    Ok(SubcommandReturnValue::Empty)
                }
            }
        }
    }
}

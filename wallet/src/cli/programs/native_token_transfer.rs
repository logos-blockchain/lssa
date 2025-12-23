use anyhow::Result;
use clap::Subcommand;
use common::transaction::NSSATransaction;
use nssa::AccountId;

use crate::{
    PrivacyPreservingAccount, WalletCore,
    cli::{
        SubcommandReturnValue, WalletSubcommand,
        programs::{ArgsReceiverMaybeUnowned, ArgsSenderOwned, ParsePrivacyPreservingAccount},
    },
    helperfunctions::{AccountPrivacyKind, parse_addr_with_privacy_prefix},
    program_facades::{
        native_token_transfer::{InitArgs, NativeBalanceToMove, NativeTokenTransfer},
        send_privacy_preserving_transaction_unified,
    },
};

/// Represents generic CLI subcommand for a wallet working with native token transfer program
#[derive(Subcommand, Debug, Clone)]
pub enum AuthTransferSubcommand {
    /// Initialize account under authenticated transfer program
    Init {
        /// account_id - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        account_id: String,
    },
    /// Send native tokens from one account to another with variable privacy
    ///
    /// If receiver is private, then `to` and (`to_npk` , `to_ipk`) is a mutually exclusive
    /// patterns.
    ///
    /// First is used for owned accounts, second otherwise.
    Send {
        #[command(flatten)]
        sender: ArgsSenderOwned,
        #[command(flatten)]
        receiver: ArgsReceiverMaybeUnowned,
        /// amount - amount of balance to move
        #[arg(long)]
        amount: u128,
    },
}

impl WalletSubcommand for AuthTransferSubcommand {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            AuthTransferSubcommand::Init { account_id } => {
                let (account_id, addr_privacy) = parse_addr_with_privacy_prefix(&account_id)?;

                match addr_privacy {
                    AccountPrivacyKind::Public => {
                        let account_id = account_id.parse()?;

                        let res = NativeTokenTransfer(wallet_core)
                            .register_account(account_id)
                            .await?;

                        println!("Results of tx send are {res:#?}");

                        let transfer_tx =
                            wallet_core.poll_native_token_transfer(res.tx_hash).await?;

                        println!("Transaction data is {transfer_tx:?}");

                        let path = wallet_core.store_persistent_data().await?;

                        println!("Stored persistent accounts at {path:#?}");
                    }
                    AccountPrivacyKind::Private => {
                        let mut account_ids = vec![];
                        let account_id: AccountId = account_id.parse()?;
                        account_ids.push(PrivacyPreservingAccount::PrivateOwned(account_id));

                        let (res, acc_decode_data) = send_privacy_preserving_transaction_unified(
                            wallet_core,
                            account_ids,
                            InitArgs {},
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
                    }
                }

                Ok(SubcommandReturnValue::Empty)
            }
            AuthTransferSubcommand::Send {
                sender,
                receiver,
                amount,
            } => {
                let from = sender.parse()?;
                let to = receiver.parse()?;

                if from.is_private() || to.is_private() {
                    let acc_vector = vec![from, to];

                    let (res, acc_decode_data) = send_privacy_preserving_transaction_unified(
                        wallet_core,
                        acc_vector,
                        NativeBalanceToMove {
                            balance_to_move: amount,
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
                    let from = from
                        .account_id()
                        .expect("Public account can not be unowned");
                    let to = to.account_id().expect("Public account can not be unowned");

                    let res = NativeTokenTransfer(wallet_core)
                        .send_public_transfer(from, to, amount)
                        .await?;

                    println!("Results of tx send are {res:#?}");

                    let transfer_tx = wallet_core.poll_native_token_transfer(res.tx_hash).await?;

                    println!("Transaction data is {transfer_tx:?}");

                    let path = wallet_core.store_persistent_data().await?;

                    println!("Stored persistent accounts at {path:#?}");

                    Ok(SubcommandReturnValue::Empty)
                }
            }
        }
    }
}

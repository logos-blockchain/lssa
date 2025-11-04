use anyhow::Result;
use clap::Subcommand;
use common::transaction::NSSATransaction;
use nssa::Address;

use crate::{
    SubcommandReturnValue, WalletCore,
    cli::WalletSubcommand,
    helperfunctions::{AddressPrivacyKind, parse_addr_with_privacy_prefix},
};

///Represents generic CLI subcommand for a wallet working with native token transfer program
#[derive(Subcommand, Debug, Clone)]
pub enum AuthTransferSubcommand {
    ///Initialize account under authenticated transfer program
    Init {
        ///addr - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        addr: String,
    },
    ///Send native tokens from one account to another with variable privacy
    ///
    ///If receiver is private, then `to` and (`to_npk` , `to_ipk`) is a mutually exclusive patterns.
    ///
    ///First is used for owned accounts, second otherwise.
    Send {
        ///from - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        from: String,
        ///to - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        to: Option<String>,
        ///to_npk - valid 32 byte hex string
        #[arg(long)]
        to_npk: Option<String>,
        ///to_ipk - valid 33 byte hex string
        #[arg(long)]
        to_ipk: Option<String>,
        ///amount - amount of balance to move
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
            AuthTransferSubcommand::Init { addr } => {
                let (addr, addr_privacy) = parse_addr_with_privacy_prefix(&addr)?;

                match addr_privacy {
                    AddressPrivacyKind::Public => {
                        let addr = addr.parse()?;

                        let res = wallet_core
                            .register_account_under_authenticated_transfers_programs(addr)
                            .await?;

                        println!("Results of tx send is {res:#?}");

                        let transfer_tx =
                            wallet_core.poll_native_token_transfer(res.tx_hash).await?;

                        println!("Transaction data is {transfer_tx:?}");

                        let path = wallet_core.store_persistent_data().await?;

                        println!("Stored persistent accounts at {path:#?}");
                    }
                    AddressPrivacyKind::Private => {
                        let addr = addr.parse()?;

                        let (res, [secret]) = wallet_core
                            .register_account_under_authenticated_transfers_programs_private(addr)
                            .await?;

                        println!("Results of tx send is {res:#?}");

                        let tx_hash = res.tx_hash;
                        let transfer_tx = wallet_core
                            .poll_native_token_transfer(tx_hash.clone())
                            .await?;

                        if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                            let acc_decode_data = vec![(secret, addr)];

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
                from,
                to,
                to_npk,
                to_ipk,
                amount,
            } => {
                let underlying_subcommand = match (to, to_npk, to_ipk) {
                    (None, None, None) => {
                        anyhow::bail!(
                            "Provide either account address of receiver or their public keys"
                        );
                    }
                    (Some(_), Some(_), Some(_)) => {
                        anyhow::bail!(
                            "Provide only one variant: either account address of receiver or their public keys"
                        );
                    }
                    (_, Some(_), None) | (_, None, Some(_)) => {
                        anyhow::bail!("List of public keys is uncomplete");
                    }
                    (Some(to), None, None) => {
                        let (from, from_privacy) = parse_addr_with_privacy_prefix(&from)?;
                        let (to, to_privacy) = parse_addr_with_privacy_prefix(&to)?;

                        match (from_privacy, to_privacy) {
                            (AddressPrivacyKind::Public, AddressPrivacyKind::Public) => {
                                NativeTokenTransferProgramSubcommand::Public { from, to, amount }
                            }
                            (AddressPrivacyKind::Private, AddressPrivacyKind::Private) => {
                                NativeTokenTransferProgramSubcommand::Private(
                                    NativeTokenTransferProgramSubcommandPrivate::PrivateOwned {
                                        from,
                                        to,
                                        amount,
                                    },
                                )
                            }
                            (AddressPrivacyKind::Private, AddressPrivacyKind::Public) => {
                                NativeTokenTransferProgramSubcommand::Deshielded {
                                    from,
                                    to,
                                    amount,
                                }
                            }
                            (AddressPrivacyKind::Public, AddressPrivacyKind::Private) => {
                                NativeTokenTransferProgramSubcommand::Shielded(
                                    NativeTokenTransferProgramSubcommandShielded::ShieldedOwned {
                                        from,
                                        to,
                                        amount,
                                    },
                                )
                            }
                        }
                    }
                    (None, Some(to_npk), Some(to_ipk)) => {
                        let (from, from_privacy) = parse_addr_with_privacy_prefix(&from)?;

                        match from_privacy {
                            AddressPrivacyKind::Private => {
                                NativeTokenTransferProgramSubcommand::Private(
                                    NativeTokenTransferProgramSubcommandPrivate::PrivateForeign {
                                        from,
                                        to_npk,
                                        to_ipk,
                                        amount,
                                    },
                                )
                            }
                            AddressPrivacyKind::Public => {
                                NativeTokenTransferProgramSubcommand::Shielded(
                                    NativeTokenTransferProgramSubcommandShielded::ShieldedForeign {
                                        from,
                                        to_npk,
                                        to_ipk,
                                        amount,
                                    },
                                )
                            }
                        }
                    }
                };

                underlying_subcommand.handle_subcommand(wallet_core).await
            }
        }
    }
}

///Represents generic CLI subcommand for a wallet working with native token transfer program
#[derive(Subcommand, Debug, Clone)]
pub enum NativeTokenTransferProgramSubcommand {
    ///Send native token transfer from `from` to `to` for `amount`
    ///
    /// Public operation
    Public {
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
    ///Private execution
    #[command(subcommand)]
    Private(NativeTokenTransferProgramSubcommandPrivate),
    ///Send native token transfer from `from` to `to` for `amount`
    ///
    /// Deshielded operation
    Deshielded {
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
    ///Shielded execution
    #[command(subcommand)]
    Shielded(NativeTokenTransferProgramSubcommandShielded),
}

///Represents generic shielded CLI subcommand for a wallet working with native token transfer program
#[derive(Subcommand, Debug, Clone)]
pub enum NativeTokenTransferProgramSubcommandShielded {
    ///Send native token transfer from `from` to `to` for `amount`
    ///
    /// Shielded operation
    ShieldedOwned {
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
    /// Shielded operation
    ShieldedForeign {
        ///from - valid 32 byte hex string
        #[arg(long)]
        from: String,
        ///to_npk - valid 32 byte hex string
        #[arg(long)]
        to_npk: String,
        ///to_ipk - valid 33 byte hex string
        #[arg(long)]
        to_ipk: String,
        ///amount - amount of balance to move
        #[arg(long)]
        amount: u128,
    },
}

///Represents generic private CLI subcommand for a wallet working with native token transfer program
#[derive(Subcommand, Debug, Clone)]
pub enum NativeTokenTransferProgramSubcommandPrivate {
    ///Send native token transfer from `from` to `to` for `amount`
    ///
    /// Private operation
    PrivateOwned {
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
    PrivateForeign {
        ///from - valid 32 byte hex string
        #[arg(long)]
        from: String,
        ///to_npk - valid 32 byte hex string
        #[arg(long)]
        to_npk: String,
        ///to_ipk - valid 33 byte hex string
        #[arg(long)]
        to_ipk: String,
        ///amount - amount of balance to move
        #[arg(long)]
        amount: u128,
    },
}

impl WalletSubcommand for NativeTokenTransferProgramSubcommandPrivate {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            NativeTokenTransferProgramSubcommandPrivate::PrivateOwned { from, to, amount } => {
                let from: Address = from.parse().unwrap();
                let to: Address = to.parse().unwrap();

                let to_initialization = wallet_core.check_private_account_initialized(&to).await?;

                let (res, [secret_from, secret_to]) = if let Some(to_proof) = to_initialization {
                    wallet_core
                        .send_private_native_token_transfer_owned_account_already_initialized(
                            from, to, amount, to_proof,
                        )
                        .await?
                } else {
                    wallet_core
                        .send_private_native_token_transfer_owned_account_not_initialized(
                            from, to, amount,
                        )
                        .await?
                };

                println!("Results of tx send is {res:#?}");

                let tx_hash = res.tx_hash;
                let transfer_tx = wallet_core
                    .poll_native_token_transfer(tx_hash.clone())
                    .await?;

                if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                    let acc_decode_data = vec![(secret_from, from), (secret_to, to)];

                    wallet_core.decode_insert_privacy_preserving_transaction_results(
                        tx,
                        &acc_decode_data,
                    )?;
                }

                let path = wallet_core.store_persistent_data().await?;

                println!("Stored persistent accounts at {path:#?}");

                Ok(SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash })
            }
            NativeTokenTransferProgramSubcommandPrivate::PrivateForeign {
                from,
                to_npk,
                to_ipk,
                amount,
            } => {
                let from: Address = from.parse().unwrap();
                let to_npk_res = hex::decode(to_npk)?;
                let mut to_npk = [0; 32];
                to_npk.copy_from_slice(&to_npk_res);
                let to_npk = nssa_core::NullifierPublicKey(to_npk);

                let to_ipk_res = hex::decode(to_ipk)?;
                let mut to_ipk = [0u8; 33];
                to_ipk.copy_from_slice(&to_ipk_res);
                let to_ipk =
                    nssa_core::encryption::shared_key_derivation::Secp256k1Point(to_ipk.to_vec());

                let (res, [secret_from, _]) = wallet_core
                    .send_private_native_token_transfer_outer_account(from, to_npk, to_ipk, amount)
                    .await?;

                println!("Results of tx send is {res:#?}");

                let tx_hash = res.tx_hash;
                let transfer_tx = wallet_core
                    .poll_native_token_transfer(tx_hash.clone())
                    .await?;

                if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                    let acc_decode_data = vec![(secret_from, from)];

                    wallet_core.decode_insert_privacy_preserving_transaction_results(
                        tx,
                        &acc_decode_data,
                    )?;
                }

                let path = wallet_core.store_persistent_data().await?;

                println!("Stored persistent accounts at {path:#?}");

                Ok(SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash })
            }
        }
    }
}

impl WalletSubcommand for NativeTokenTransferProgramSubcommandShielded {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            NativeTokenTransferProgramSubcommandShielded::ShieldedOwned { from, to, amount } => {
                let from: Address = from.parse().unwrap();
                let to: Address = to.parse().unwrap();

                let to_initialization = wallet_core.check_private_account_initialized(&to).await?;

                let (res, [secret]) = if let Some(to_proof) = to_initialization {
                    wallet_core
                        .send_shielded_native_token_transfer_already_initialized(
                            from, to, amount, to_proof,
                        )
                        .await?
                } else {
                    wallet_core
                        .send_shielded_native_token_transfer_not_initialized(from, to, amount)
                        .await?
                };

                println!("Results of tx send is {res:#?}");

                let tx_hash = res.tx_hash;
                let transfer_tx = wallet_core
                    .poll_native_token_transfer(tx_hash.clone())
                    .await?;

                if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                    let acc_decode_data = vec![(secret, to)];

                    wallet_core.decode_insert_privacy_preserving_transaction_results(
                        tx,
                        &acc_decode_data,
                    )?;
                }

                let path = wallet_core.store_persistent_data().await?;

                println!("Stored persistent accounts at {path:#?}");

                Ok(SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash })
            }
            NativeTokenTransferProgramSubcommandShielded::ShieldedForeign {
                from,
                to_npk,
                to_ipk,
                amount,
            } => {
                let from: Address = from.parse().unwrap();

                let to_npk_res = hex::decode(to_npk)?;
                let mut to_npk = [0; 32];
                to_npk.copy_from_slice(&to_npk_res);
                let to_npk = nssa_core::NullifierPublicKey(to_npk);

                let to_ipk_res = hex::decode(to_ipk)?;
                let mut to_ipk = [0u8; 33];
                to_ipk.copy_from_slice(&to_ipk_res);
                let to_ipk =
                    nssa_core::encryption::shared_key_derivation::Secp256k1Point(to_ipk.to_vec());

                let res = wallet_core
                    .send_shielded_native_token_transfer_outer_account(from, to_npk, to_ipk, amount)
                    .await?;

                println!("Results of tx send is {res:#?}");

                let tx_hash = res.tx_hash;

                let path = wallet_core.store_persistent_data().await?;

                println!("Stored persistent accounts at {path:#?}");

                Ok(SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash })
            }
        }
    }
}

impl WalletSubcommand for NativeTokenTransferProgramSubcommand {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            NativeTokenTransferProgramSubcommand::Private(private_subcommand) => {
                private_subcommand.handle_subcommand(wallet_core).await
            }
            NativeTokenTransferProgramSubcommand::Shielded(shielded_subcommand) => {
                shielded_subcommand.handle_subcommand(wallet_core).await
            }
            NativeTokenTransferProgramSubcommand::Deshielded { from, to, amount } => {
                let from: Address = from.parse().unwrap();
                let to: Address = to.parse().unwrap();

                let (res, [secret]) = wallet_core
                    .send_deshielded_native_token_transfer(from, to, amount)
                    .await?;

                println!("Results of tx send is {res:#?}");

                let tx_hash = res.tx_hash;
                let transfer_tx = wallet_core
                    .poll_native_token_transfer(tx_hash.clone())
                    .await?;

                if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                    let acc_decode_data = vec![(secret, from)];

                    wallet_core.decode_insert_privacy_preserving_transaction_results(
                        tx,
                        &acc_decode_data,
                    )?;
                }

                let path = wallet_core.store_persistent_data().await?;

                println!("Stored persistent accounts at {path:#?}");

                Ok(SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash })
            }
            NativeTokenTransferProgramSubcommand::Public { from, to, amount } => {
                let from: Address = from.parse().unwrap();
                let to: Address = to.parse().unwrap();

                let res = wallet_core
                    .send_public_native_token_transfer(from, to, amount)
                    .await?;

                println!("Results of tx send is {res:#?}");

                let transfer_tx = wallet_core.poll_native_token_transfer(res.tx_hash).await?;

                println!("Transaction data is {transfer_tx:?}");

                let path = wallet_core.store_persistent_data().await?;

                println!("Stored persistent accounts at {path:#?}");

                Ok(SubcommandReturnValue::Empty)
            }
        }
    }
}

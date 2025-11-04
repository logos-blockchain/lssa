use anyhow::Result;
use clap::Subcommand;
use common::transaction::NSSATransaction;
use nssa::Address;

use crate::{
    SubcommandReturnValue, WalletCore,
    cli::WalletSubcommand,
    helperfunctions::{AddressPrivacyKind, parse_addr_with_privacy_prefix},
};

///Represents generic CLI subcommand for a wallet working with token program
#[derive(Subcommand, Debug, Clone)]
pub enum TokenProgramAgnosticSubcommand {
    ///Produce a new token
    ///
    ///Currently the only supported privacy options is for public definition
    New {
        ///definition_addr - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        definition_addr: String,
        ///supply_addr - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        supply_addr: String,
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        total_supply: u128,
    },
    ///Send tokens from one account to another with variable privacy
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

impl WalletSubcommand for TokenProgramAgnosticSubcommand {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            TokenProgramAgnosticSubcommand::New {
                definition_addr,
                supply_addr,
                name,
                total_supply,
            } => {
                let (definition_addr, definition_addr_privacy) =
                    parse_addr_with_privacy_prefix(&definition_addr)?;
                let (supply_addr, supply_addr_privacy) =
                    parse_addr_with_privacy_prefix(&supply_addr)?;

                let underlying_subcommand = match (definition_addr_privacy, supply_addr_privacy) {
                    (AddressPrivacyKind::Public, AddressPrivacyKind::Public) => {
                        TokenProgramSubcommand::Public(
                            TokenProgramSubcommandPublic::CreateNewToken {
                                definition_addr,
                                supply_addr,
                                name,
                                total_supply,
                            },
                        )
                    }
                    (AddressPrivacyKind::Public, AddressPrivacyKind::Private) => {
                        TokenProgramSubcommand::Private(
                            TokenProgramSubcommandPrivate::CreateNewTokenPrivateOwned {
                                definition_addr,
                                supply_addr,
                                name,
                                total_supply,
                            },
                        )
                    }
                    (AddressPrivacyKind::Private, AddressPrivacyKind::Private) => {
                        //ToDo: maybe implement this one. It is not immediately clear why definition should be private.
                        anyhow::bail!("Unavailable privacy pairing")
                    }
                    (AddressPrivacyKind::Private, AddressPrivacyKind::Public) => {
                        //ToDo: Probably valid. If definition is not public, but supply is it is very suspicious.
                        anyhow::bail!("Unavailable privacy pairing")
                    }
                };

                underlying_subcommand.handle_subcommand(wallet_core).await
            }
            TokenProgramAgnosticSubcommand::Send {
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
                                TokenProgramSubcommand::Public(
                                    TokenProgramSubcommandPublic::TransferToken {
                                        sender_addr: from,
                                        recipient_addr: to,
                                        balance_to_move: amount,
                                    },
                                )
                            }
                            (AddressPrivacyKind::Private, AddressPrivacyKind::Private) => {
                                TokenProgramSubcommand::Private(
                                    TokenProgramSubcommandPrivate::TransferTokenPrivateOwned {
                                        sender_addr: from,
                                        recipient_addr: to,
                                        balance_to_move: amount,
                                    },
                                )
                            }
                            (AddressPrivacyKind::Private, AddressPrivacyKind::Public) => {
                                TokenProgramSubcommand::Deshielded(
                                    TokenProgramSubcommandDeshielded::TransferTokenDeshielded {
                                        sender_addr: from,
                                        recipient_addr: to,
                                        balance_to_move: amount,
                                    },
                                )
                            }
                            (AddressPrivacyKind::Public, AddressPrivacyKind::Private) => {
                                TokenProgramSubcommand::Shielded(
                                    TokenProgramSubcommandShielded::TransferTokenShieldedOwned {
                                        sender_addr: from,
                                        recipient_addr: to,
                                        balance_to_move: amount,
                                    },
                                )
                            }
                        }
                    }
                    (None, Some(to_npk), Some(to_ipk)) => {
                        let (from, from_privacy) = parse_addr_with_privacy_prefix(&from)?;

                        match from_privacy {
                            AddressPrivacyKind::Private => TokenProgramSubcommand::Private(
                                TokenProgramSubcommandPrivate::TransferTokenPrivateForeign {
                                    sender_addr: from,
                                    recipient_npk: to_npk,
                                    recipient_ipk: to_ipk,
                                    balance_to_move: amount,
                                },
                            ),
                            AddressPrivacyKind::Public => TokenProgramSubcommand::Shielded(
                                TokenProgramSubcommandShielded::TransferTokenShieldedForeign {
                                    sender_addr: from,
                                    recipient_npk: to_npk,
                                    recipient_ipk: to_ipk,
                                    balance_to_move: amount,
                                },
                            ),
                        }
                    }
                };

                underlying_subcommand.handle_subcommand(wallet_core).await
            }
        }
    }
}

///Represents generic CLI subcommand for a wallet working with token_program
#[derive(Subcommand, Debug, Clone)]
pub enum TokenProgramSubcommand {
    ///Public execution
    #[command(subcommand)]
    Public(TokenProgramSubcommandPublic),
    ///Private execution
    #[command(subcommand)]
    Private(TokenProgramSubcommandPrivate),
    ///Deshielded execution
    #[command(subcommand)]
    Deshielded(TokenProgramSubcommandDeshielded),
    ///Shielded execution
    #[command(subcommand)]
    Shielded(TokenProgramSubcommandShielded),
}

///Represents generic public CLI subcommand for a wallet working with token_program
#[derive(Subcommand, Debug, Clone)]
pub enum TokenProgramSubcommandPublic {
    //Create a new token using the token program
    CreateNewToken {
        #[arg(short, long)]
        definition_addr: String,
        #[arg(short, long)]
        supply_addr: String,
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        total_supply: u128,
    },
    //Transfer tokens using the token program
    TransferToken {
        #[arg(short, long)]
        sender_addr: String,
        #[arg(short, long)]
        recipient_addr: String,
        #[arg(short, long)]
        balance_to_move: u128,
    },
}

///Represents generic private CLI subcommand for a wallet working with token_program
#[derive(Subcommand, Debug, Clone)]
pub enum TokenProgramSubcommandPrivate {
    //Create a new token using the token program
    CreateNewTokenPrivateOwned {
        #[arg(short, long)]
        definition_addr: String,
        #[arg(short, long)]
        supply_addr: String,
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        total_supply: u128,
    },
    //Transfer tokens using the token program
    TransferTokenPrivateOwned {
        #[arg(short, long)]
        sender_addr: String,
        #[arg(short, long)]
        recipient_addr: String,
        #[arg(short, long)]
        balance_to_move: u128,
    },
    //Transfer tokens using the token program
    TransferTokenPrivateForeign {
        #[arg(short, long)]
        sender_addr: String,
        ///recipient_npk - valid 32 byte hex string
        #[arg(long)]
        recipient_npk: String,
        ///recipient_ipk - valid 33 byte hex string
        #[arg(long)]
        recipient_ipk: String,
        #[arg(short, long)]
        balance_to_move: u128,
    },
}

///Represents deshielded public CLI subcommand for a wallet working with token_program
#[derive(Subcommand, Debug, Clone)]
pub enum TokenProgramSubcommandDeshielded {
    //Transfer tokens using the token program
    TransferTokenDeshielded {
        #[arg(short, long)]
        sender_addr: String,
        #[arg(short, long)]
        recipient_addr: String,
        #[arg(short, long)]
        balance_to_move: u128,
    },
}

///Represents generic shielded CLI subcommand for a wallet working with token_program
#[derive(Subcommand, Debug, Clone)]
pub enum TokenProgramSubcommandShielded {
    //Transfer tokens using the token program
    TransferTokenShieldedOwned {
        #[arg(short, long)]
        sender_addr: String,
        #[arg(short, long)]
        recipient_addr: String,
        #[arg(short, long)]
        balance_to_move: u128,
    },
    //Transfer tokens using the token program
    TransferTokenShieldedForeign {
        #[arg(short, long)]
        sender_addr: String,
        ///recipient_npk - valid 32 byte hex string
        #[arg(long)]
        recipient_npk: String,
        ///recipient_ipk - valid 33 byte hex string
        #[arg(long)]
        recipient_ipk: String,
        #[arg(short, long)]
        balance_to_move: u128,
    },
}

impl WalletSubcommand for TokenProgramSubcommandPublic {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            TokenProgramSubcommandPublic::CreateNewToken {
                definition_addr,
                supply_addr,
                name,
                total_supply,
            } => {
                let name = name.as_bytes();
                if name.len() > 6 {
                    // TODO: return error
                    panic!();
                }
                let mut name_bytes = [0; 6];
                name_bytes[..name.len()].copy_from_slice(name);
                wallet_core
                    .send_new_token_definition(
                        definition_addr.parse().unwrap(),
                        supply_addr.parse().unwrap(),
                        name_bytes,
                        total_supply,
                    )
                    .await?;
                Ok(SubcommandReturnValue::Empty)
            }
            TokenProgramSubcommandPublic::TransferToken {
                sender_addr,
                recipient_addr,
                balance_to_move,
            } => {
                wallet_core
                    .send_transfer_token_transaction(
                        sender_addr.parse().unwrap(),
                        recipient_addr.parse().unwrap(),
                        balance_to_move,
                    )
                    .await?;
                Ok(SubcommandReturnValue::Empty)
            }
        }
    }
}

impl WalletSubcommand for TokenProgramSubcommandPrivate {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            TokenProgramSubcommandPrivate::CreateNewTokenPrivateOwned {
                definition_addr,
                supply_addr,
                name,
                total_supply,
            } => {
                let name = name.as_bytes();
                if name.len() > 6 {
                    // TODO: return error
                    panic!("Name length mismatch");
                }
                let mut name_bytes = [0; 6];
                name_bytes[..name.len()].copy_from_slice(name);

                let definition_addr: Address = definition_addr.parse().unwrap();
                let supply_addr: Address = supply_addr.parse().unwrap();

                let (res, [secret_supply]) = wallet_core
                    .send_new_token_definition_private_owned(
                        definition_addr,
                        supply_addr,
                        name_bytes,
                        total_supply,
                    )
                    .await?;

                println!("Results of tx send is {res:#?}");

                let tx_hash = res.tx_hash;
                let transfer_tx = wallet_core
                    .poll_native_token_transfer(tx_hash.clone())
                    .await?;

                if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                    let acc_decode_data = vec![(secret_supply, supply_addr)];

                    wallet_core.decode_insert_privacy_preserving_transaction_results(
                        tx,
                        &acc_decode_data,
                    )?;
                }

                let path = wallet_core.store_persistent_data().await?;

                println!("Stored persistent accounts at {path:#?}");

                Ok(SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash })
            }
            TokenProgramSubcommandPrivate::TransferTokenPrivateOwned {
                sender_addr,
                recipient_addr,
                balance_to_move,
            } => {
                let sender_addr: Address = sender_addr.parse().unwrap();
                let recipient_addr: Address = recipient_addr.parse().unwrap();

                let recipient_initialization = wallet_core
                    .check_private_account_initialized(&recipient_addr)
                    .await?;

                let (res, [secret_sender, secret_recipient]) =
                    if let Some(recipient_proof) = recipient_initialization {
                        wallet_core
                        .send_transfer_token_transaction_private_owned_account_already_initialized(
                            sender_addr,
                            recipient_addr,
                            balance_to_move,
                            recipient_proof,
                        )
                        .await?
                    } else {
                        wallet_core
                            .send_transfer_token_transaction_private_owned_account_not_initialized(
                                sender_addr,
                                recipient_addr,
                                balance_to_move,
                            )
                            .await?
                    };

                println!("Results of tx send is {res:#?}");

                let tx_hash = res.tx_hash;
                let transfer_tx = wallet_core
                    .poll_native_token_transfer(tx_hash.clone())
                    .await?;

                if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                    let acc_decode_data = vec![
                        (secret_sender, sender_addr),
                        (secret_recipient, recipient_addr),
                    ];

                    wallet_core.decode_insert_privacy_preserving_transaction_results(
                        tx,
                        &acc_decode_data,
                    )?;
                }

                let path = wallet_core.store_persistent_data().await?;

                println!("Stored persistent accounts at {path:#?}");

                Ok(SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash })
            }
            TokenProgramSubcommandPrivate::TransferTokenPrivateForeign {
                sender_addr,
                recipient_npk,
                recipient_ipk,
                balance_to_move,
            } => {
                let sender_addr: Address = sender_addr.parse().unwrap();
                let recipient_npk_res = hex::decode(recipient_npk)?;
                let mut recipient_npk = [0; 32];
                recipient_npk.copy_from_slice(&recipient_npk_res);
                let recipient_npk = nssa_core::NullifierPublicKey(recipient_npk);

                let recipient_ipk_res = hex::decode(recipient_ipk)?;
                let mut recipient_ipk = [0u8; 33];
                recipient_ipk.copy_from_slice(&recipient_ipk_res);
                let recipient_ipk = nssa_core::encryption::shared_key_derivation::Secp256k1Point(
                    recipient_ipk.to_vec(),
                );

                let (res, [secret_sender, _]) = wallet_core
                    .send_transfer_token_transaction_private_foreign_account(
                        sender_addr,
                        recipient_npk,
                        recipient_ipk,
                        balance_to_move,
                    )
                    .await?;

                println!("Results of tx send is {res:#?}");

                let tx_hash = res.tx_hash;
                let transfer_tx = wallet_core
                    .poll_native_token_transfer(tx_hash.clone())
                    .await?;

                if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                    let acc_decode_data = vec![(secret_sender, sender_addr)];

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

impl WalletSubcommand for TokenProgramSubcommandDeshielded {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            TokenProgramSubcommandDeshielded::TransferTokenDeshielded {
                sender_addr,
                recipient_addr,
                balance_to_move,
            } => {
                let sender_addr: Address = sender_addr.parse().unwrap();
                let recipient_addr: Address = recipient_addr.parse().unwrap();

                let (res, [secret_sender]) = wallet_core
                    .send_transfer_token_transaction_deshielded(
                        sender_addr,
                        recipient_addr,
                        balance_to_move,
                    )
                    .await?;

                println!("Results of tx send is {res:#?}");

                let tx_hash = res.tx_hash;
                let transfer_tx = wallet_core
                    .poll_native_token_transfer(tx_hash.clone())
                    .await?;

                if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                    let acc_decode_data = vec![(secret_sender, sender_addr)];

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

impl WalletSubcommand for TokenProgramSubcommandShielded {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            TokenProgramSubcommandShielded::TransferTokenShieldedForeign {
                sender_addr,
                recipient_npk,
                recipient_ipk,
                balance_to_move,
            } => {
                let sender_addr: Address = sender_addr.parse().unwrap();
                let recipient_npk_res = hex::decode(recipient_npk)?;
                let mut recipient_npk = [0; 32];
                recipient_npk.copy_from_slice(&recipient_npk_res);
                let recipient_npk = nssa_core::NullifierPublicKey(recipient_npk);

                let recipient_ipk_res = hex::decode(recipient_ipk)?;
                let mut recipient_ipk = [0u8; 33];
                recipient_ipk.copy_from_slice(&recipient_ipk_res);
                let recipient_ipk = nssa_core::encryption::shared_key_derivation::Secp256k1Point(
                    recipient_ipk.to_vec(),
                );

                let res = wallet_core
                    .send_transfer_token_transaction_shielded_foreign_account(
                        sender_addr,
                        recipient_npk,
                        recipient_ipk,
                        balance_to_move,
                    )
                    .await?;

                println!("Results of tx send is {res:#?}");

                let tx_hash = res.tx_hash;
                let transfer_tx = wallet_core
                    .poll_native_token_transfer(tx_hash.clone())
                    .await?;

                if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                    println!("Transaction data is {:?}", tx.message);
                }

                let path = wallet_core.store_persistent_data().await?;

                println!("Stored persistent accounts at {path:#?}");

                Ok(SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash })
            }
            TokenProgramSubcommandShielded::TransferTokenShieldedOwned {
                sender_addr,
                recipient_addr,
                balance_to_move,
            } => {
                let sender_addr: Address = sender_addr.parse().unwrap();
                let recipient_addr: Address = recipient_addr.parse().unwrap();

                let recipient_initialization = wallet_core
                    .check_private_account_initialized(&recipient_addr)
                    .await?;

                let (res, [secret_recipient]) =
                    if let Some(recipient_proof) = recipient_initialization {
                        wallet_core
                        .send_transfer_token_transaction_shielded_owned_account_already_initialized(
                            sender_addr,
                            recipient_addr,
                            balance_to_move,
                            recipient_proof,
                        )
                        .await?
                    } else {
                        wallet_core
                            .send_transfer_token_transaction_shielded_owned_account_not_initialized(
                                sender_addr,
                                recipient_addr,
                                balance_to_move,
                            )
                            .await?
                    };

                println!("Results of tx send is {res:#?}");

                let tx_hash = res.tx_hash;
                let transfer_tx = wallet_core
                    .poll_native_token_transfer(tx_hash.clone())
                    .await?;

                if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                    let acc_decode_data = vec![(secret_recipient, recipient_addr)];

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

impl WalletSubcommand for TokenProgramSubcommand {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            TokenProgramSubcommand::Private(private_subcommand) => {
                private_subcommand.handle_subcommand(wallet_core).await
            }
            TokenProgramSubcommand::Public(public_subcommand) => {
                public_subcommand.handle_subcommand(wallet_core).await
            }
            TokenProgramSubcommand::Deshielded(deshielded_subcommand) => {
                deshielded_subcommand.handle_subcommand(wallet_core).await
            }
            TokenProgramSubcommand::Shielded(shielded_subcommand) => {
                shielded_subcommand.handle_subcommand(wallet_core).await
            }
        }
    }
}

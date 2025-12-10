use anyhow::Result;
use clap::Subcommand;
use common::transaction::NSSATransaction;
use nssa::AccountId;

use crate::{
    WalletCore,
    cli::{SubcommandReturnValue, WalletSubcommand},
    helperfunctions::{AccountPrivacyKind, parse_addr_with_privacy_prefix},
    program_facades::token::Token,
};

/// Represents generic CLI subcommand for a wallet working with token program
#[derive(Subcommand, Debug, Clone)]
pub enum TokenProgramAgnosticSubcommand {
    /// Produce a new token
    New {
        /// definition_account_id - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        definition_account_id: String,
        /// supply_account_id - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        supply_account_id: String,
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
        /// from - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        from: String,
        /// to - valid 32 byte base58 string with privacy prefix
        #[arg(long)]
        to: Option<String>,
        /// to_npk - valid 32 byte hex string
        #[arg(long)]
        to_npk: Option<String>,
        /// to_ipk - valid 33 byte hex string
        #[arg(long)]
        to_ipk: Option<String>,
        /// amount - amount of balance to move
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
                definition_account_id,
                supply_account_id,
                name,
                total_supply,
            } => {
                let (definition_account_id, definition_addr_privacy) =
                    parse_addr_with_privacy_prefix(&definition_account_id)?;
                let (supply_account_id, supply_addr_privacy) =
                    parse_addr_with_privacy_prefix(&supply_account_id)?;

                let underlying_subcommand = match (definition_addr_privacy, supply_addr_privacy) {
                    (AccountPrivacyKind::Public, AccountPrivacyKind::Public) => {
                        TokenProgramSubcommand::Create(
                            CreateNewTokenProgramSubcommand::NewPublicDefPublicSupp {
                                definition_account_id,
                                supply_account_id,
                                name,
                                total_supply,
                            },
                        )
                    }
                    (AccountPrivacyKind::Public, AccountPrivacyKind::Private) => {
                        TokenProgramSubcommand::Create(
                            CreateNewTokenProgramSubcommand::NewPublicDefPrivateSupp {
                                definition_account_id,
                                supply_account_id,
                                name,
                                total_supply,
                            },
                        )
                    }
                    (AccountPrivacyKind::Private, AccountPrivacyKind::Private) => {
                        TokenProgramSubcommand::Create(
                            CreateNewTokenProgramSubcommand::NewPrivateDefPrivateSupp {
                                definition_account_id,
                                supply_account_id,
                                name,
                                total_supply,
                            },
                        )
                    }
                    (AccountPrivacyKind::Private, AccountPrivacyKind::Public) => {
                        TokenProgramSubcommand::Create(
                            CreateNewTokenProgramSubcommand::NewPrivateDefPublicSupp {
                                definition_account_id,
                                supply_account_id,
                                name,
                                total_supply,
                            },
                        )
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
                            "Provide either account account_id of receiver or their public keys"
                        );
                    }
                    (Some(_), Some(_), Some(_)) => {
                        anyhow::bail!(
                            "Provide only one variant: either account account_id of receiver or their public keys"
                        );
                    }
                    (_, Some(_), None) | (_, None, Some(_)) => {
                        anyhow::bail!("List of public keys is uncomplete");
                    }
                    (Some(to), None, None) => {
                        let (from, from_privacy) = parse_addr_with_privacy_prefix(&from)?;
                        let (to, to_privacy) = parse_addr_with_privacy_prefix(&to)?;

                        match (from_privacy, to_privacy) {
                            (AccountPrivacyKind::Public, AccountPrivacyKind::Public) => {
                                TokenProgramSubcommand::Public(
                                    TokenProgramSubcommandPublic::TransferToken {
                                        sender_account_id: from,
                                        recipient_account_id: to,
                                        balance_to_move: amount,
                                    },
                                )
                            }
                            (AccountPrivacyKind::Private, AccountPrivacyKind::Private) => {
                                TokenProgramSubcommand::Private(
                                    TokenProgramSubcommandPrivate::TransferTokenPrivateOwned {
                                        sender_account_id: from,
                                        recipient_account_id: to,
                                        balance_to_move: amount,
                                    },
                                )
                            }
                            (AccountPrivacyKind::Private, AccountPrivacyKind::Public) => {
                                TokenProgramSubcommand::Deshielded(
                                    TokenProgramSubcommandDeshielded::TransferTokenDeshielded {
                                        sender_account_id: from,
                                        recipient_account_id: to,
                                        balance_to_move: amount,
                                    },
                                )
                            }
                            (AccountPrivacyKind::Public, AccountPrivacyKind::Private) => {
                                TokenProgramSubcommand::Shielded(
                                    TokenProgramSubcommandShielded::TransferTokenShieldedOwned {
                                        sender_account_id: from,
                                        recipient_account_id: to,
                                        balance_to_move: amount,
                                    },
                                )
                            }
                        }
                    }
                    (None, Some(to_npk), Some(to_ipk)) => {
                        let (from, from_privacy) = parse_addr_with_privacy_prefix(&from)?;

                        match from_privacy {
                            AccountPrivacyKind::Private => TokenProgramSubcommand::Private(
                                TokenProgramSubcommandPrivate::TransferTokenPrivateForeign {
                                    sender_account_id: from,
                                    recipient_npk: to_npk,
                                    recipient_ipk: to_ipk,
                                    balance_to_move: amount,
                                },
                            ),
                            AccountPrivacyKind::Public => TokenProgramSubcommand::Shielded(
                                TokenProgramSubcommandShielded::TransferTokenShieldedForeign {
                                    sender_account_id: from,
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

/// Represents generic CLI subcommand for a wallet working with token_program
#[derive(Subcommand, Debug, Clone)]
pub enum TokenProgramSubcommand {
    /// Creation of new token
    #[command(subcommand)]
    Create(CreateNewTokenProgramSubcommand),
    /// Public execution
    #[command(subcommand)]
    Public(TokenProgramSubcommandPublic),
    /// Private execution
    #[command(subcommand)]
    Private(TokenProgramSubcommandPrivate),
    /// Deshielded execution
    #[command(subcommand)]
    Deshielded(TokenProgramSubcommandDeshielded),
    /// Shielded execution
    #[command(subcommand)]
    Shielded(TokenProgramSubcommandShielded),
}

/// Represents generic public CLI subcommand for a wallet working with token_program
#[derive(Subcommand, Debug, Clone)]
pub enum TokenProgramSubcommandPublic {
    // Transfer tokens using the token program
    TransferToken {
        #[arg(short, long)]
        sender_account_id: String,
        #[arg(short, long)]
        recipient_account_id: String,
        #[arg(short, long)]
        balance_to_move: u128,
    },
}

/// Represents generic private CLI subcommand for a wallet working with token_program
#[derive(Subcommand, Debug, Clone)]
pub enum TokenProgramSubcommandPrivate {
    // Transfer tokens using the token program
    TransferTokenPrivateOwned {
        #[arg(short, long)]
        sender_account_id: String,
        #[arg(short, long)]
        recipient_account_id: String,
        #[arg(short, long)]
        balance_to_move: u128,
    },
    // Transfer tokens using the token program
    TransferTokenPrivateForeign {
        #[arg(short, long)]
        sender_account_id: String,
        /// recipient_npk - valid 32 byte hex string
        #[arg(long)]
        recipient_npk: String,
        /// recipient_ipk - valid 33 byte hex string
        #[arg(long)]
        recipient_ipk: String,
        #[arg(short, long)]
        balance_to_move: u128,
    },
}

/// Represents deshielded public CLI subcommand for a wallet working with token_program
#[derive(Subcommand, Debug, Clone)]
pub enum TokenProgramSubcommandDeshielded {
    // Transfer tokens using the token program
    TransferTokenDeshielded {
        #[arg(short, long)]
        sender_account_id: String,
        #[arg(short, long)]
        recipient_account_id: String,
        #[arg(short, long)]
        balance_to_move: u128,
    },
}

/// Represents generic shielded CLI subcommand for a wallet working with token_program
#[derive(Subcommand, Debug, Clone)]
pub enum TokenProgramSubcommandShielded {
    // Transfer tokens using the token program
    TransferTokenShieldedOwned {
        #[arg(short, long)]
        sender_account_id: String,
        #[arg(short, long)]
        recipient_account_id: String,
        #[arg(short, long)]
        balance_to_move: u128,
    },
    // Transfer tokens using the token program
    TransferTokenShieldedForeign {
        #[arg(short, long)]
        sender_account_id: String,
        /// recipient_npk - valid 32 byte hex string
        #[arg(long)]
        recipient_npk: String,
        /// recipient_ipk - valid 33 byte hex string
        #[arg(long)]
        recipient_ipk: String,
        #[arg(short, long)]
        balance_to_move: u128,
    },
}

/// Represents generic initialization subcommand for a wallet working with token_program
#[derive(Subcommand, Debug, Clone)]
pub enum CreateNewTokenProgramSubcommand {
    /// Create a new token using the token program
    ///
    /// Definition - public, supply - public
    NewPublicDefPublicSupp {
        #[arg(short, long)]
        definition_account_id: String,
        #[arg(short, long)]
        supply_account_id: String,
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        total_supply: u128,
    },
    /// Create a new token using the token program
    ///
    /// Definition - public, supply - private
    NewPublicDefPrivateSupp {
        #[arg(short, long)]
        definition_account_id: String,
        #[arg(short, long)]
        supply_account_id: String,
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        total_supply: u128,
    },
    /// Create a new token using the token program
    ///
    /// Definition - private, supply - public
    NewPrivateDefPublicSupp {
        #[arg(short, long)]
        definition_account_id: String,
        #[arg(short, long)]
        supply_account_id: String,
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        total_supply: u128,
    },
    /// Create a new token using the token program
    ///
    /// Definition - private, supply - private
    NewPrivateDefPrivateSupp {
        #[arg(short, long)]
        definition_account_id: String,
        #[arg(short, long)]
        supply_account_id: String,
        #[arg(short, long)]
        name: String,
        #[arg(short, long)]
        total_supply: u128,
    },
}

impl WalletSubcommand for TokenProgramSubcommandPublic {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            TokenProgramSubcommandPublic::TransferToken {
                sender_account_id,
                recipient_account_id,
                balance_to_move,
            } => {
                Token(wallet_core)
                    .send_transfer_transaction(
                        sender_account_id.parse().unwrap(),
                        recipient_account_id.parse().unwrap(),
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
            TokenProgramSubcommandPrivate::TransferTokenPrivateOwned {
                sender_account_id,
                recipient_account_id,
                balance_to_move,
            } => {
                let sender_account_id: AccountId = sender_account_id.parse().unwrap();
                let recipient_account_id: AccountId = recipient_account_id.parse().unwrap();

                let (res, [secret_sender, secret_recipient]) = Token(wallet_core)
                    .send_transfer_transaction_private_owned_account(
                        sender_account_id,
                        recipient_account_id,
                        balance_to_move,
                    )
                    .await?;

                println!("Results of tx send are {res:#?}");

                let tx_hash = res.tx_hash;
                let transfer_tx = wallet_core
                    .poll_native_token_transfer(tx_hash.clone())
                    .await?;

                if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                    let acc_decode_data = vec![
                        (secret_sender, sender_account_id),
                        (secret_recipient, recipient_account_id),
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
                sender_account_id,
                recipient_npk,
                recipient_ipk,
                balance_to_move,
            } => {
                let sender_account_id: AccountId = sender_account_id.parse().unwrap();
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

                let (res, [secret_sender, _]) = Token(wallet_core)
                    .send_transfer_transaction_private_foreign_account(
                        sender_account_id,
                        recipient_npk,
                        recipient_ipk,
                        balance_to_move,
                    )
                    .await?;

                println!("Results of tx send are {res:#?}");

                let tx_hash = res.tx_hash;
                let transfer_tx = wallet_core
                    .poll_native_token_transfer(tx_hash.clone())
                    .await?;

                if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                    let acc_decode_data = vec![(secret_sender, sender_account_id)];

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
                sender_account_id,
                recipient_account_id,
                balance_to_move,
            } => {
                let sender_account_id: AccountId = sender_account_id.parse().unwrap();
                let recipient_account_id: AccountId = recipient_account_id.parse().unwrap();

                let (res, secret_sender) = Token(wallet_core)
                    .send_transfer_transaction_deshielded(
                        sender_account_id,
                        recipient_account_id,
                        balance_to_move,
                    )
                    .await?;

                println!("Results of tx send are {res:#?}");

                let tx_hash = res.tx_hash;
                let transfer_tx = wallet_core
                    .poll_native_token_transfer(tx_hash.clone())
                    .await?;

                if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                    let acc_decode_data = vec![(secret_sender, sender_account_id)];

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
                sender_account_id,
                recipient_npk,
                recipient_ipk,
                balance_to_move,
            } => {
                let sender_account_id: AccountId = sender_account_id.parse().unwrap();
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

                let (res, _) = Token(wallet_core)
                    .send_transfer_transaction_shielded_foreign_account(
                        sender_account_id,
                        recipient_npk,
                        recipient_ipk,
                        balance_to_move,
                    )
                    .await?;

                println!("Results of tx send are {res:#?}");

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
                sender_account_id,
                recipient_account_id,
                balance_to_move,
            } => {
                let sender_account_id: AccountId = sender_account_id.parse().unwrap();
                let recipient_account_id: AccountId = recipient_account_id.parse().unwrap();

                let (res, secret_recipient) = Token(wallet_core)
                    .send_transfer_transaction_shielded_owned_account(
                        sender_account_id,
                        recipient_account_id,
                        balance_to_move,
                    )
                    .await?;

                println!("Results of tx send are {res:#?}");

                let tx_hash = res.tx_hash;
                let transfer_tx = wallet_core
                    .poll_native_token_transfer(tx_hash.clone())
                    .await?;

                if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                    let acc_decode_data = vec![(secret_recipient, recipient_account_id)];

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

impl WalletSubcommand for CreateNewTokenProgramSubcommand {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            CreateNewTokenProgramSubcommand::NewPrivateDefPrivateSupp {
                definition_account_id,
                supply_account_id,
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

                let definition_account_id: AccountId = definition_account_id.parse().unwrap();
                let supply_account_id: AccountId = supply_account_id.parse().unwrap();

                let (res, [secret_definition, secret_supply]) = Token(wallet_core)
                    .send_new_definition_private_owned_definiton_and_supply(
                        definition_account_id,
                        supply_account_id,
                        name_bytes,
                        total_supply,
                    )
                    .await?;

                println!("Results of tx send are {res:#?}");

                let tx_hash = res.tx_hash;
                let transfer_tx = wallet_core
                    .poll_native_token_transfer(tx_hash.clone())
                    .await?;

                if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                    let acc_decode_data = vec![
                        (secret_definition, definition_account_id),
                        (secret_supply, supply_account_id),
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
            CreateNewTokenProgramSubcommand::NewPrivateDefPublicSupp {
                definition_account_id,
                supply_account_id,
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

                let definition_account_id: AccountId = definition_account_id.parse().unwrap();
                let supply_account_id: AccountId = supply_account_id.parse().unwrap();

                let (res, secret_definition) = Token(wallet_core)
                    .send_new_definition_private_owned_definiton(
                        definition_account_id,
                        supply_account_id,
                        name_bytes,
                        total_supply,
                    )
                    .await?;

                println!("Results of tx send are {res:#?}");

                let tx_hash = res.tx_hash;
                let transfer_tx = wallet_core
                    .poll_native_token_transfer(tx_hash.clone())
                    .await?;

                if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                    let acc_decode_data = vec![(secret_definition, definition_account_id)];

                    wallet_core.decode_insert_privacy_preserving_transaction_results(
                        tx,
                        &acc_decode_data,
                    )?;
                }

                let path = wallet_core.store_persistent_data().await?;

                println!("Stored persistent accounts at {path:#?}");

                Ok(SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash })
            }
            CreateNewTokenProgramSubcommand::NewPublicDefPrivateSupp {
                definition_account_id,
                supply_account_id,
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

                let definition_account_id: AccountId = definition_account_id.parse().unwrap();
                let supply_account_id: AccountId = supply_account_id.parse().unwrap();

                let (res, secret_supply) = Token(wallet_core)
                    .send_new_definition_private_owned_supply(
                        definition_account_id,
                        supply_account_id,
                        name_bytes,
                        total_supply,
                    )
                    .await?;

                println!("Results of tx send are {res:#?}");

                let tx_hash = res.tx_hash;
                let transfer_tx = wallet_core
                    .poll_native_token_transfer(tx_hash.clone())
                    .await?;

                if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                    let acc_decode_data = vec![(secret_supply, supply_account_id)];

                    wallet_core.decode_insert_privacy_preserving_transaction_results(
                        tx,
                        &acc_decode_data,
                    )?;
                }

                let path = wallet_core.store_persistent_data().await?;

                println!("Stored persistent accounts at {path:#?}");

                Ok(SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash })
            }
            CreateNewTokenProgramSubcommand::NewPublicDefPublicSupp {
                definition_account_id,
                supply_account_id,
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
                Token(wallet_core)
                    .send_new_definition(
                        definition_account_id.parse().unwrap(),
                        supply_account_id.parse().unwrap(),
                        name_bytes,
                        total_supply,
                    )
                    .await?;
                Ok(SubcommandReturnValue::Empty)
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
            TokenProgramSubcommand::Create(creation_subcommand) => {
                creation_subcommand.handle_subcommand(wallet_core).await
            }
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

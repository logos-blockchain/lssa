use std::str::FromStr;

use anyhow::Result;
use clap::Subcommand;
use common::transaction::NSSATransaction;
use nssa::Address;

use crate::{
    SubcommandReturnValue, WalletCore, cli::WalletSubcommand, helperfunctions::HumanReadableAccount,
};

///Represents generic chain CLI subcommand
#[derive(Subcommand, Debug, Clone)]
pub enum AccountSubcommand {
    ///Get
    #[command(subcommand)]
    Get(GetSubcommand),
    ///Fetch
    #[command(subcommand)]
    Fetch(FetchSubcommand),
    ///Register
    #[command(subcommand)]
    Register(RegisterSubcommand),
}

///Represents generic getter CLI subcommand
#[derive(Subcommand, Debug, Clone)]
pub enum GetSubcommand {
    ///Get account `addr` balance
    PublicAccountBalance {
        #[arg(short, long)]
        addr: String,
    },
    ///Get account `addr` nonce
    PublicAccountNonce {
        #[arg(short, long)]
        addr: String,
    },
    ///Get account at address `addr`
    PublicAccount {
        #[arg(short, long)]
        addr: String,
    },
    ///Get private account with `addr` from storage
    PrivateAccount {
        #[arg(short, long)]
        addr: String,
    },
}

///Represents generic getter CLI subcommand
#[derive(Subcommand, Debug, Clone)]
pub enum FetchSubcommand {
    ///Fetch transaction by `hash`
    Tx {
        #[arg(short, long)]
        tx_hash: String,
    },
    ///Claim account `acc_addr` generated in transaction `tx_hash`, using secret `sh_secret` at ciphertext id `ciph_id`
    PrivateAccount {
        ///tx_hash - valid 32 byte hex string
        #[arg(long)]
        tx_hash: String,
        ///acc_addr - valid 32 byte hex string
        #[arg(long)]
        acc_addr: String,
        ///output_id - id of the output in the transaction
        #[arg(long)]
        output_id: usize,
    },
}

///Represents generic register CLI subcommand
#[derive(Subcommand, Debug, Clone)]
pub enum RegisterSubcommand {
    ///Register new public account
    Public {},
    ///Register new private account
    Private {},
}

impl WalletSubcommand for GetSubcommand {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            GetSubcommand::PublicAccountBalance { addr } => {
                let addr = Address::from_str(&addr)?;

                let balance = wallet_core.get_account_balance(addr).await?;
                println!("Accounts {addr} balance is {balance}");

                Ok(SubcommandReturnValue::Empty)
            }
            GetSubcommand::PublicAccountNonce { addr } => {
                let addr = Address::from_str(&addr)?;

                let nonce = wallet_core.get_accounts_nonces(vec![addr]).await?[0];
                println!("Accounts {addr} nonce is {nonce}");

                Ok(SubcommandReturnValue::Empty)
            }
            GetSubcommand::PublicAccount { addr } => {
                let addr: Address = addr.parse()?;
                let account = wallet_core.get_account_public(addr).await?;
                let account_hr: HumanReadableAccount = account.clone().into();
                println!("{}", serde_json::to_string(&account_hr).unwrap());

                Ok(SubcommandReturnValue::Account(account))
            }
            GetSubcommand::PrivateAccount { addr } => {
                let addr: Address = addr.parse()?;
                if let Some(account) = wallet_core.get_account_private(&addr) {
                    println!("{}", serde_json::to_string(&account).unwrap());
                } else {
                    println!("Private account not found.");
                }
                Ok(SubcommandReturnValue::Empty)
            }
        }
    }
}

impl WalletSubcommand for FetchSubcommand {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            FetchSubcommand::Tx { tx_hash } => {
                let tx_obj = wallet_core
                    .sequencer_client
                    .get_transaction_by_hash(tx_hash)
                    .await?;

                println!("Transaction object {tx_obj:#?}");

                Ok(SubcommandReturnValue::Empty)
            }
            FetchSubcommand::PrivateAccount {
                tx_hash,
                acc_addr,
                output_id: ciph_id,
            } => {
                let acc_addr: Address = acc_addr.parse().unwrap();

                let account_key_chain = wallet_core
                    .storage
                    .user_data
                    .user_private_accounts
                    .get(&acc_addr);

                let Some((account_key_chain, _)) = account_key_chain else {
                    anyhow::bail!("Account not found");
                };

                let transfer_tx = wallet_core.poll_native_token_transfer(tx_hash).await?;

                if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                    let to_ebc = tx.message.encrypted_private_post_states[ciph_id].clone();
                    let to_comm = tx.message.new_commitments[ciph_id].clone();
                    let shared_secret =
                        account_key_chain.calculate_shared_secret_receiver(to_ebc.epk);

                    let res_acc_to = nssa_core::EncryptionScheme::decrypt(
                        &to_ebc.ciphertext,
                        &shared_secret,
                        &to_comm,
                        ciph_id as u32,
                    )
                    .unwrap();

                    println!("RES acc to {res_acc_to:#?}");

                    println!("Transaction data is {:?}", tx.message);

                    wallet_core
                        .storage
                        .insert_private_account_data(acc_addr, res_acc_to);
                }

                let path = wallet_core.store_persistent_accounts().await?;

                println!("Stored persistent accounts at {path:#?}");

                Ok(SubcommandReturnValue::Empty)
            }
        }
    }
}

impl WalletSubcommand for RegisterSubcommand {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            RegisterSubcommand::Public {} => {
                let addr = wallet_core.create_new_account_public();

                println!("Generated new account with addr {addr}");

                let path = wallet_core.store_persistent_accounts().await?;

                println!("Stored persistent accounts at {path:#?}");

                Ok(SubcommandReturnValue::RegisterAccount { addr })
            }
            RegisterSubcommand::Private {} => {
                let addr = wallet_core.create_new_account_private();

                let (key, _) = wallet_core
                    .storage
                    .user_data
                    .get_private_account(&addr)
                    .unwrap();

                println!("Generated new account with addr {addr}");
                println!("With npk {}", hex::encode(&key.nullifer_public_key));
                println!(
                    "With ipk {}",
                    hex::encode(key.incoming_viewing_public_key.to_bytes())
                );

                let path = wallet_core.store_persistent_accounts().await?;

                println!("Stored persistent accounts at {path:#?}");

                Ok(SubcommandReturnValue::RegisterAccount { addr })
            }
        }
    }
}

impl WalletSubcommand for AccountSubcommand {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            AccountSubcommand::Get(get_subcommand) => {
                get_subcommand.handle_subcommand(wallet_core).await
            }
            AccountSubcommand::Fetch(fetch_subcommand) => {
                fetch_subcommand.handle_subcommand(wallet_core).await
            }
            AccountSubcommand::Register(register_subcommand) => {
                register_subcommand.handle_subcommand(wallet_core).await
            }
        }
    }
}

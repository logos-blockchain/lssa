use std::{fs::File, io::Write, path::PathBuf, str::FromStr, sync::Arc};

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use common::{
    ExecutionFailureKind,
    sequencer_client::{SequencerClient, json::SendTxResponse},
    transaction::{EncodedTransaction, NSSATransaction},
};

use anyhow::Result;
use chain_storage::WalletChainStore;
use config::WalletConfig;
use log::info;
use nssa::{Account, Address};

use clap::{Parser, Subcommand};
use nssa_core::Commitment;

use crate::{
    helperfunctions::{
        HumanReadableAccount, fetch_config, fetch_persistent_accounts, get_home,
        produce_data_for_storage,
    },
    poller::TxPoller,
};

//

pub const HOME_DIR_ENV_VAR: &str = "NSSA_WALLET_HOME_DIR";

pub mod chain_storage;
pub mod config;
pub mod helperfunctions;
pub mod poller;
pub mod token_transfers;

pub struct WalletCore {
    pub storage: WalletChainStore,
    pub poller: TxPoller,
    pub sequencer_client: Arc<SequencerClient>,
}

impl WalletCore {
    pub fn start_from_config_update_chain(config: WalletConfig) -> Result<Self> {
        let client = Arc::new(SequencerClient::new(config.sequencer_addr.clone())?);
        let tx_poller = TxPoller::new(config.clone(), client.clone());

        let mut storage = WalletChainStore::new(config)?;

        let persistent_accounts = fetch_persistent_accounts()?;
        for pers_acc_data in persistent_accounts {
            storage.insert_account_data(pers_acc_data);
        }

        Ok(Self {
            storage,
            poller: tx_poller,
            sequencer_client: client.clone(),
        })
    }

    ///Store persistent accounts at home
    pub fn store_persistent_accounts(&self) -> Result<PathBuf> {
        let home = get_home()?;
        let accs_path = home.join("curr_accounts.json");

        let data = produce_data_for_storage(&self.storage.user_data);
        let accs = serde_json::to_vec_pretty(&data)?;

        let mut accs_file = File::create(accs_path.as_path())?;
        accs_file.write_all(&accs)?;

        info!("Stored accounts data at {accs_path:#?}");

        Ok(accs_path)
    }

    pub fn create_new_account_public(&mut self) -> Address {
        self.storage
            .user_data
            .generate_new_public_transaction_private_key()
    }

    pub fn create_new_account_private(&mut self) -> Address {
        self.storage
            .user_data
            .generate_new_privacy_preserving_transaction_key_chain()
    }

    pub async fn claim_pinata(
        &self,
        pinata_addr: Address,
        winner_addr: Address,
        solution: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let addresses = vec![pinata_addr, winner_addr];
        let program_id = nssa::program::Program::pinata().id();
        let message =
            nssa::public_transaction::Message::try_new(program_id, addresses, vec![], solution)
                .unwrap();

        let witness_set = nssa::public_transaction::WitnessSet::for_message(&message, &[]);
        let tx = nssa::PublicTransaction::new(message, witness_set);

        Ok(self.sequencer_client.send_tx_public(tx).await?)
    }

    pub async fn send_new_token_definition(
        &self,
        definition_address: Address,
        supply_address: Address,
        name: [u8; 6],
        total_supply: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let addresses = vec![definition_address, supply_address];
        let program_id = nssa::program::Program::token().id();
        // Instruction must be: [0x00 || total_supply (little-endian 16 bytes) || name (6 bytes)]
        let mut instruction = [0; 23];
        instruction[1..17].copy_from_slice(&total_supply.to_le_bytes());
        instruction[17..].copy_from_slice(&name);
        let message =
            nssa::public_transaction::Message::try_new(program_id, addresses, vec![], instruction)
                .unwrap();

        let witness_set = nssa::public_transaction::WitnessSet::for_message(&message, &[]);

        let tx = nssa::PublicTransaction::new(message, witness_set);

        Ok(self.sequencer_client.send_tx_public(tx).await?)
    }

    pub async fn send_transfer_token_transaction(
        &self,
        sender_address: Address,
        recipient_address: Address,
        amount: u128,
    ) -> Result<SendTxResponse, ExecutionFailureKind> {
        let addresses = vec![sender_address, recipient_address];
        let program_id = nssa::program::Program::token().id();
        // Instruction must be: [0x01 || amount (little-endian 16 bytes) || 0x00 || 0x00 || 0x00 || 0x00 || 0x00 || 0x00].
        let mut instruction = [0; 23];
        instruction[0] = 0x01;
        instruction[1..17].copy_from_slice(&amount.to_le_bytes());
        let Ok(nonces) = self.get_accounts_nonces(vec![sender_address]).await else {
            return Err(ExecutionFailureKind::SequencerError);
        };
        let message =
            nssa::public_transaction::Message::try_new(program_id, addresses, nonces, instruction)
                .unwrap();

        let Some(signing_key) = self
            .storage
            .user_data
            .get_pub_account_signing_key(&sender_address)
        else {
            return Err(ExecutionFailureKind::KeyNotFoundError);
        };
        let witness_set =
            nssa::public_transaction::WitnessSet::for_message(&message, &[signing_key]);

        let tx = nssa::PublicTransaction::new(message, witness_set);

        Ok(self.sequencer_client.send_tx_public(tx).await?)
    }
    ///Get account balance
    pub async fn get_account_balance(&self, acc: Address) -> Result<u128> {
        Ok(self
            .sequencer_client
            .get_account_balance(acc.to_string())
            .await?
            .balance)
    }

    ///Get accounts nonces
    pub async fn get_accounts_nonces(&self, accs: Vec<Address>) -> Result<Vec<u128>> {
        Ok(self
            .sequencer_client
            .get_accounts_nonces(accs.into_iter().map(|acc| acc.to_string()).collect())
            .await?
            .nonces)
    }

    ///Get account
    pub async fn get_account_public(&self, addr: Address) -> Result<Account> {
        let response = self.sequencer_client.get_account(addr.to_string()).await?;
        Ok(response.account)
    }

    pub fn get_account_private(&self, addr: &Address) -> Option<Account> {
        self.storage
            .user_data
            .user_private_accounts
            .get(addr)
            .map(|value| value.1.clone())
    }

    pub fn get_private_account_commitment(&self, addr: &Address) -> Option<Commitment> {
        let (keys, account) = self.storage.user_data.user_private_accounts.get(addr)?;
        Some(Commitment::new(&keys.nullifer_public_key, account))
    }

    ///Poll transactions
    pub async fn poll_native_token_transfer(&self, hash: String) -> Result<NSSATransaction> {
        let transaction_encoded = self.poller.poll_tx(hash).await?;
        let tx_base64_decode = BASE64.decode(transaction_encoded)?;
        let pub_tx = borsh::from_slice::<EncodedTransaction>(&tx_base64_decode).unwrap();

        Ok(NSSATransaction::try_from(&pub_tx)?)
    }
}

///Represents CLI command for a wallet
#[derive(Subcommand, Debug, Clone)]
#[clap(about)]
pub enum Command {
    ///Send native token transfer from `from` to `to` for `amount`
    ///
    /// Public operation
    SendNativeTokenTransferPublic {
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
    SendNativeTokenTransferPrivateOwnedAccount {
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
    SendNativeTokenTransferPrivateForeignAccount {
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
    ///Send native token transfer from `from` to `to` for `amount`
    ///
    /// Deshielded operation
    SendNativeTokenTransferDeshielded {
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
    SendNativeTokenTransferShielded {
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
    SendNativeTokenTransferShieldedForeignAccount {
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
    ///Claim account `acc_addr` generated in transaction `tx_hash`, using secret `sh_secret` at ciphertext id `ciph_id`
    FetchPrivateAccount {
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
    ///Get private account with `addr` from storage
    GetPrivateAccount {
        #[arg(short, long)]
        addr: String,
    },
    ///Register new public account
    RegisterAccountPublic {},
    ///Register new private account
    RegisterAccountPrivate {},
    ///Fetch transaction by `hash`
    FetchTx {
        #[arg(short, long)]
        tx_hash: String,
    },
    ///Get account `addr` balance
    GetPublicAccountBalance {
        #[arg(short, long)]
        addr: String,
    },
    ///Get account `addr` nonce
    GetPublicAccountNonce {
        #[arg(short, long)]
        addr: String,
    },
    ///Get account at address `addr`
    GetPublicAccount {
        #[arg(short, long)]
        addr: String,
    },
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
    // TODO: Testnet only. Refactor to prevent compilation on mainnet.
    // Claim piñata prize
    ClaimPinata {
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

///To execute commands, env var NSSA_WALLET_HOME_DIR must be set into directory with config
#[derive(Parser, Debug)]
#[clap(version, about)]
pub struct Args {
    /// Wallet command
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Clone)]
pub enum SubcommandReturnValue {
    PrivacyPreservingTransfer { tx_hash: String },
    RegisterAccount { addr: nssa::Address },
    Account(nssa::Account),
    Empty,
}

pub async fn execute_subcommand(command: Command) -> Result<SubcommandReturnValue> {
    let wallet_config = fetch_config()?;
    let mut wallet_core = WalletCore::start_from_config_update_chain(wallet_config)?;

    let subcommand_ret = match command {
        Command::SendNativeTokenTransferPublic { from, to, amount } => {
            let from: Address = from.parse().unwrap();
            let to: Address = to.parse().unwrap();

            let res = wallet_core
                .send_public_native_token_transfer(from, to, amount)
                .await?;

            println!("Results of tx send is {res:#?}");

            let transfer_tx = wallet_core.poll_native_token_transfer(res.tx_hash).await?;

            println!("Transaction data is {transfer_tx:?}");

            let path = wallet_core.store_persistent_accounts()?;

            println!("Stored persistent accounts at {path:#?}");

            SubcommandReturnValue::Empty
        }
        Command::SendNativeTokenTransferPrivateOwnedAccount { from, to, amount } => {
            let from: Address = from.parse().unwrap();
            let to: Address = to.parse().unwrap();

            let (res, [secret_from, secret_to]) = wallet_core
                .send_private_native_token_transfer_owned_account(from, to, amount)
                .await?;

            println!("Results of tx send is {res:#?}");

            let tx_hash = res.tx_hash;
            let transfer_tx = wallet_core
                .poll_native_token_transfer(tx_hash.clone())
                .await?;

            if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                let from_ebc = tx.message.encrypted_private_post_states[0].clone();
                let from_comm = tx.message.new_commitments[0].clone();

                let to_ebc = tx.message.encrypted_private_post_states[1].clone();
                let to_comm = tx.message.new_commitments[1].clone();

                let res_acc_from = nssa_core::EncryptionScheme::decrypt(
                    &from_ebc.ciphertext,
                    &secret_from,
                    &from_comm,
                    0,
                )
                .unwrap();

                let res_acc_to = nssa_core::EncryptionScheme::decrypt(
                    &to_ebc.ciphertext,
                    &secret_to,
                    &to_comm,
                    1,
                )
                .unwrap();

                println!("Received new from acc {res_acc_from:#?}");
                println!("Received new to acc {res_acc_to:#?}");

                println!("Transaction data is {:?}", tx.message);

                wallet_core
                    .storage
                    .insert_private_account_data(from, res_acc_from);
                wallet_core
                    .storage
                    .insert_private_account_data(to, res_acc_to);
            }

            let path = wallet_core.store_persistent_accounts()?;

            println!("Stored persistent accounts at {path:#?}");

            SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash }
        }
        Command::SendNativeTokenTransferPrivateForeignAccount {
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

            let (res, [secret_from, secret_to]) = wallet_core
                .send_private_native_token_transfer_outer_account(from, to_npk, to_ipk, amount)
                .await?;

            println!("Results of tx send is {res:#?}");

            let tx_hash = res.tx_hash;
            let transfer_tx = wallet_core
                .poll_native_token_transfer(tx_hash.clone())
                .await?;

            if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                let from_ebc = tx.message.encrypted_private_post_states[0].clone();
                let from_comm = tx.message.new_commitments[0].clone();

                let to_ebc = tx.message.encrypted_private_post_states[1].clone();
                let to_comm = tx.message.new_commitments[1].clone();

                let res_acc_from = nssa_core::EncryptionScheme::decrypt(
                    &from_ebc.ciphertext,
                    &secret_from,
                    &from_comm,
                    0,
                )
                .unwrap();

                let res_acc_to = nssa_core::EncryptionScheme::decrypt(
                    &to_ebc.ciphertext,
                    &secret_to,
                    &to_comm,
                    1,
                )
                .unwrap();

                println!("RES acc {res_acc_from:#?}");
                println!("RES acc to {res_acc_to:#?}");

                println!("Transaction data is {:?}", tx.message);

                wallet_core
                    .storage
                    .insert_private_account_data(from, res_acc_from);
            }

            let path = wallet_core.store_persistent_accounts()?;

            println!("Stored persistent accounts at {path:#?}");

            SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash }
        }
        Command::SendNativeTokenTransferDeshielded { from, to, amount } => {
            let from: Address = from.parse().unwrap();
            let to: Address = to.parse().unwrap();

            let (res, secret) = wallet_core
                .send_deshielded_native_token_transfer(from, to, amount)
                .await?;

            println!("Results of tx send is {res:#?}");

            let tx_hash = res.tx_hash;
            let transfer_tx = wallet_core
                .poll_native_token_transfer(tx_hash.clone())
                .await?;

            if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                let from_ebc = tx.message.encrypted_private_post_states[0].clone();
                let from_comm = tx.message.new_commitments[0].clone();

                let res_acc_from = nssa_core::EncryptionScheme::decrypt(
                    &from_ebc.ciphertext,
                    &secret,
                    &from_comm,
                    0,
                )
                .unwrap();

                println!("RES acc {res_acc_from:#?}");

                println!("Transaction data is {:?}", tx.message);

                wallet_core
                    .storage
                    .insert_private_account_data(from, res_acc_from);
            }

            let path = wallet_core.store_persistent_accounts()?;

            println!("Stored persistent accounts at {path:#?}");

            SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash }
        }
        Command::SendNativeTokenTransferShielded { from, to, amount } => {
            let from: Address = from.parse().unwrap();
            let to: Address = to.parse().unwrap();

            let (res, secret) = wallet_core
                .send_shielded_native_token_transfer(from, to, amount)
                .await?;

            println!("Results of tx send is {res:#?}");

            let tx_hash = res.tx_hash;
            let transfer_tx = wallet_core
                .poll_native_token_transfer(tx_hash.clone())
                .await?;

            if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                let to_ebc = tx.message.encrypted_private_post_states[0].clone();
                let to_comm = tx.message.new_commitments[0].clone();

                let res_acc_to =
                    nssa_core::EncryptionScheme::decrypt(&to_ebc.ciphertext, &secret, &to_comm, 0)
                        .unwrap();

                println!("RES acc to {res_acc_to:#?}");

                println!("Transaction data is {:?}", tx.message);
            }

            let path = wallet_core.store_persistent_accounts()?;

            println!("Stored persistent accounts at {path:#?}");

            SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash }
        }
        Command::SendNativeTokenTransferShieldedForeignAccount {
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

            let (res, secret) = wallet_core
                .send_shielded_native_token_transfer_outer_account(from, to_npk, to_ipk, amount)
                .await?;

            println!("Results of tx send is {res:#?}");

            let tx_hash = res.tx_hash;
            let transfer_tx = wallet_core
                .poll_native_token_transfer(tx_hash.clone())
                .await?;

            if let NSSATransaction::PrivacyPreserving(tx) = transfer_tx {
                let to_ebc = tx.message.encrypted_private_post_states[0].clone();
                let to_comm = tx.message.new_commitments[0].clone();

                let res_acc_to =
                    nssa_core::EncryptionScheme::decrypt(&to_ebc.ciphertext, &secret, &to_comm, 0)
                        .unwrap();

                println!("RES acc to {res_acc_to:#?}");

                println!("Transaction data is {:?}", tx.message);
            }

            let path = wallet_core.store_persistent_accounts()?;

            println!("Stored persistent accounts at {path:#?}");

            SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash }
        }
        Command::FetchPrivateAccount {
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
                let shared_secret = account_key_chain.calculate_shared_secret_receiver(to_ebc.epk);

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

            let path = wallet_core.store_persistent_accounts()?;

            println!("Stored persistent accounts at {path:#?}");

            SubcommandReturnValue::Empty
        }
        Command::RegisterAccountPublic {} => {
            let addr = wallet_core.create_new_account_public();

            println!("Generated new account with addr {addr}");

            let path = wallet_core.store_persistent_accounts()?;

            println!("Stored persistent accounts at {path:#?}");

            SubcommandReturnValue::RegisterAccount { addr }
        }
        Command::RegisterAccountPrivate {} => {
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

            let path = wallet_core.store_persistent_accounts()?;

            println!("Stored persistent accounts at {path:#?}");

            SubcommandReturnValue::RegisterAccount { addr }
        }
        Command::FetchTx { tx_hash } => {
            let tx_obj = wallet_core
                .sequencer_client
                .get_transaction_by_hash(tx_hash)
                .await?;

            println!("Transaction object {tx_obj:#?}");

            SubcommandReturnValue::Empty
        }
        Command::GetPublicAccountBalance { addr } => {
            let addr = Address::from_str(&addr)?;

            let balance = wallet_core.get_account_balance(addr).await?;
            println!("Accounts {addr} balance is {balance}");

            SubcommandReturnValue::Empty
        }
        Command::GetPublicAccountNonce { addr } => {
            let addr = Address::from_str(&addr)?;

            let nonce = wallet_core.get_accounts_nonces(vec![addr]).await?[0];
            println!("Accounts {addr} nonce is {nonce}");

            SubcommandReturnValue::Empty
        }
        Command::GetPublicAccount { addr } => {
            let addr: Address = addr.parse()?;
            let account = wallet_core.get_account_public(addr).await?;
            let account_hr: HumanReadableAccount = account.clone().into();
            println!("{}", serde_json::to_string(&account_hr).unwrap());

            SubcommandReturnValue::Account(account)
        }
        Command::GetPrivateAccount { addr } => {
            let addr: Address = addr.parse()?;
            if let Some(account) = wallet_core.get_account_private(&addr) {
                println!("{}", serde_json::to_string(&account).unwrap());
            } else {
                println!("Private account not found.");
            }
            SubcommandReturnValue::Empty
        }
        Command::CreateNewToken {
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
            SubcommandReturnValue::Empty
        }
        Command::TransferToken {
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
            SubcommandReturnValue::Empty
        }
        Command::ClaimPinata {
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

            SubcommandReturnValue::Empty
        }
    };

    Ok(subcommand_ret)
}

use std::{fs::File, io::Write, path::PathBuf, str::FromStr, sync::Arc};

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use common::{
    sequencer_client::SequencerClient,
    transaction::{EncodedTransaction, NSSATransaction},
};

use anyhow::Result;
use chain_storage::WalletChainStore;
use config::WalletConfig;
use log::info;
use nssa::{Account, Address, program::Program};

use clap::{Parser, Subcommand};
use nssa_core::{Commitment, MembershipProof};

use crate::cli::WalletSubcommand;
use crate::{
    cli::token_program::TokenProgramSubcommand,
    helperfunctions::{
        HumanReadableAccount, fetch_config, fetch_persistent_accounts, get_home,
        produce_data_for_storage,
    },
    poller::TxPoller,
};

pub const HOME_DIR_ENV_VAR: &str = "NSSA_WALLET_HOME_DIR";

pub mod chain_storage;
pub mod cli;
pub mod config;
pub mod helperfunctions;
pub mod pinata_interactions;
pub mod poller;
pub mod token_program_interactions;
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

    pub async fn check_private_account_initialized(
        &self,
        addr: &Address,
    ) -> Result<Option<MembershipProof>> {
        if let Some(acc_comm) = self.get_private_account_commitment(addr) {
            self.sequencer_client
                .get_proof_for_commitment(acc_comm)
                .await
                .map_err(anyhow::Error::from)
        } else {
            Ok(None)
        }
    }

    pub fn decode_insert_privacy_preserving_transaction_results(
        &mut self,
        tx: nssa::privacy_preserving_transaction::PrivacyPreservingTransaction,
        acc_decode_data: &[(nssa_core::SharedSecretKey, Address)],
    ) -> Result<()> {
        for (output_index, (secret, acc_address)) in acc_decode_data.iter().enumerate() {
            let acc_ead = tx.message.encrypted_private_post_states[output_index].clone();
            let acc_comm = tx.message.new_commitments[output_index].clone();

            let res_acc = nssa_core::EncryptionScheme::decrypt(
                &acc_ead.ciphertext,
                secret,
                &acc_comm,
                output_index as u32,
            )
            .unwrap();

            println!("Received new acc {res_acc:#?}");

            self.storage
                .insert_private_account_data(*acc_address, res_acc);
        }

        println!("Transaction data is {:?}", tx.message);

        Ok(())
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
    AuthenticatedTransferInitializePublicAccount {},
    // Check the wallet can connect to the node and builtin local programs
    // match the remote versions
    CheckHealth {},
    // TODO: Testnet only. Refactor to prevent compilation on mainnet.
    // Claim piñata prize
    ClaimPinataPrivateReceiverOwned {
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
    ///Token command
    #[command(subcommand)]
    TokenProgram(TokenProgramSubcommand),
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
                let acc_decode_data = vec![(secret_from, from), (secret_to, to)];

                wallet_core
                    .decode_insert_privacy_preserving_transaction_results(tx, &acc_decode_data)?;
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

                wallet_core
                    .decode_insert_privacy_preserving_transaction_results(tx, &acc_decode_data)?;
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
                let acc_decode_data = vec![(secret, from)];

                wallet_core
                    .decode_insert_privacy_preserving_transaction_results(tx, &acc_decode_data)?;
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
                let acc_decode_data = vec![(secret, to)];

                wallet_core
                    .decode_insert_privacy_preserving_transaction_results(tx, &acc_decode_data)?;
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

            let (res, _) = wallet_core
                .send_shielded_native_token_transfer_outer_account(from, to_npk, to_ipk, amount)
                .await?;

            println!("Results of tx send is {res:#?}");

            let tx_hash = res.tx_hash;

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
                let account_hr: HumanReadableAccount = account.into();
                println!("{}", serde_json::to_string(&account_hr).unwrap());
            } else {
                println!("Private account not found.");
            }
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
        Command::ClaimPinataPrivateReceiverOwned {
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

                wallet_core
                    .decode_insert_privacy_preserving_transaction_results(tx, &acc_decode_data)?;
            }

            let path = wallet_core.store_persistent_accounts()?;

            println!("Stored persistent accounts at {path:#?}");

            SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash }
        }
        Command::AuthenticatedTransferInitializePublicAccount {} => {
            let addr = wallet_core.create_new_account_public();

            println!("Generated new account with addr {addr}");

            let path = wallet_core.store_persistent_accounts()?;

            println!("Stored persistent accounts at {path:#?}");

            let res = wallet_core
                .register_account_under_authenticated_transfers_programs(addr)
                .await?;

            println!("Results of tx send is {res:#?}");

            let _transfer_tx = wallet_core.poll_native_token_transfer(res.tx_hash).await?;

            SubcommandReturnValue::RegisterAccount { addr }
        }
        Command::TokenProgram(token_subcommand) => {
            token_subcommand.handle_subcommand(&mut wallet_core).await?
        }
    };

    Ok(subcommand_ret)
}

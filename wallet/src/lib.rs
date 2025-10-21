use std::{path::PathBuf, sync::Arc};

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use common::{
    block::HashableBlockData,
    sequencer_client::SequencerClient,
    transaction::{EncodedTransaction, NSSATransaction},
};

use anyhow::Result;
use chain_storage::WalletChainStore;
use config::WalletConfig;
use log::info;
use nssa::{
    Account, Address, privacy_preserving_transaction::message::EncryptedAccountData,
    program::Program,
};

use clap::{Parser, Subcommand};
use nssa_core::{Commitment, MembershipProof};
use tokio::io::AsyncWriteExt;

use crate::cli::{
    WalletSubcommand, account::AccountSubcommand, chain::ChainSubcommand,
    native_token_transfer_program::NativeTokenTransferProgramSubcommand,
    pinata_program::PinataProgramSubcommand,
};
use crate::{
    cli::token_program::TokenProgramSubcommand,
    helperfunctions::{
        fetch_config, fetch_persistent_accounts, get_home, produce_data_for_storage,
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
    pub async fn start_from_config_update_chain(config: WalletConfig) -> Result<Self> {
        let client = Arc::new(SequencerClient::new(config.sequencer_addr.clone())?);
        let tx_poller = TxPoller::new(config.clone(), client.clone());

        let mut storage = WalletChainStore::new(config)?;

        let persistent_accounts = fetch_persistent_accounts().await?;
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
    pub async fn store_persistent_accounts(&self) -> Result<PathBuf> {
        let home = get_home()?;
        let accs_path = home.join("curr_accounts.json");

        let data = produce_data_for_storage(&self.storage.user_data);
        let accs = serde_json::to_vec_pretty(&data)?;

        let mut accs_file = tokio::fs::File::create(accs_path.as_path()).await?;
        accs_file.write_all(&accs).await?;

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
    ///Transfer command
    #[command(subcommand)]
    Transfer(NativeTokenTransferProgramSubcommand),
    ///Chain command
    #[command(subcommand)]
    Chain(ChainSubcommand),
    ///Chain command
    #[command(subcommand)]
    Account(AccountSubcommand),
    ///Pinata command
    #[command(subcommand)]
    PinataProgram(PinataProgramSubcommand),
    ///Token command
    #[command(subcommand)]
    TokenProgram(TokenProgramSubcommand),
    AuthenticatedTransferInitializePublicAccount {},
    // Check the wallet can connect to the node and builtin local programs
    // match the remote versions
    CheckHealth {},
}

///To execute commands, env var NSSA_WALLET_HOME_DIR must be set into directory with config
#[derive(Parser, Debug)]
#[clap(version, about)]
pub struct Args {
    /// Continious run flag
    #[arg(short, long)]
    pub continious_run: bool,
    /// Wallet command
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Clone)]
pub enum SubcommandReturnValue {
    PrivacyPreservingTransfer { tx_hash: String },
    RegisterAccount { addr: nssa::Address },
    Account(nssa::Account),
    Empty,
}

pub async fn execute_subcommand(command: Command) -> Result<SubcommandReturnValue> {
    let wallet_config = fetch_config().await?;
    let mut wallet_core = WalletCore::start_from_config_update_chain(wallet_config).await?;

    let subcommand_ret = match command {
        Command::Transfer(transfer_subcommand) => {
            transfer_subcommand
                .handle_subcommand(&mut wallet_core)
                .await?
        }
        Command::Chain(chain_subcommand) => {
            chain_subcommand.handle_subcommand(&mut wallet_core).await?
        }
        Command::Account(account_subcommand) => {
            account_subcommand
                .handle_subcommand(&mut wallet_core)
                .await?
        }
        Command::PinataProgram(pinata_subcommand) => {
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
        Command::AuthenticatedTransferInitializePublicAccount {} => {
            let addr = wallet_core.create_new_account_public();

            println!("Generated new account with addr {addr}");

            let path = wallet_core.store_persistent_accounts().await?;

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

pub async fn execute_continious_run() -> Result<()> {
    let config = fetch_config().await?;
    let seq_client = Arc::new(SequencerClient::new(config.sequencer_addr.clone())?);
    let mut wallet_core = WalletCore::start_from_config_update_chain(config.clone()).await?;

    let mut latest_block_num = seq_client.get_last_block().await?.last_block;
    let mut curr_last_block = latest_block_num;

    loop {
        for block_id in curr_last_block..(latest_block_num + 1) {
            let block = borsh::from_slice::<HashableBlockData>(
                &seq_client.get_block(block_id).await?.block,
            )?;

            for tx in block.transactions {
                let nssa_tx = NSSATransaction::try_from(&tx)?;

                if let NSSATransaction::PrivacyPreserving(tx) = nssa_tx {
                    let mut affected_accounts = vec![];

                    for (acc_addr, (key_chain, _)) in
                        &wallet_core.storage.user_data.user_private_accounts
                    {
                        let view_tag = EncryptedAccountData::compute_view_tag(
                            key_chain.nullifer_public_key.clone(),
                            key_chain.incoming_viewing_public_key.clone(),
                        );

                        for (ciph_id, encrypted_data) in tx
                            .message()
                            .encrypted_private_post_states
                            .iter()
                            .enumerate()
                        {
                            if encrypted_data.view_tag == view_tag {
                                let ciphertext = &encrypted_data.ciphertext;
                                let commitment = &tx.message.new_commitments[ciph_id];
                                let shared_secret = key_chain
                                    .calculate_shared_secret_receiver(encrypted_data.epk.clone());

                                let res_acc = nssa_core::EncryptionScheme::decrypt(
                                    ciphertext,
                                    &shared_secret,
                                    commitment,
                                    ciph_id as u32,
                                );

                                if let Some(res_acc) = res_acc {
                                    println!(
                                        "Received new account for addr {acc_addr:#?} with account object {res_acc:#?}"
                                    );

                                    affected_accounts.push((*acc_addr, res_acc));
                                }
                            }
                        }
                    }

                    for (affected_addr, new_acc) in affected_accounts {
                        wallet_core
                            .storage
                            .insert_private_account_data(affected_addr, new_acc);
                    }
                }
            }

            wallet_core.store_persistent_accounts().await?;

            println!(
                "Block at id {block_id} with timestamp {} parsed",
                block.timestamp
            );
        }

        curr_last_block = latest_block_num + 1;

        tokio::time::sleep(std::time::Duration::from_millis(
            config.seq_poll_timeout_millis,
        ))
        .await;

        latest_block_num = seq_client.get_last_block().await?.last_block;
    }
}

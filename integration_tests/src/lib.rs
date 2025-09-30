use std::{path::PathBuf, time::Duration};

use actix_web::dev::ServerHandle;
use anyhow::Result;
use clap::Parser;
use common::sequencer_client::SequencerClient;
use log::{info, warn};
use nssa::program::Program;
use nssa_core::{NullifierPublicKey, encryption::shared_key_derivation::Secp256k1Point};
use sequencer_core::config::SequencerConfig;
use sequencer_runner::startup_sequencer;
use tempfile::TempDir;
use tokio::task::JoinHandle;
use wallet::{
    Command, SubcommandReturnValue, WalletCore,
    config::PersistentAccountData,
    helperfunctions::{fetch_config, fetch_persistent_accounts, produce_account_addr_from_hex},
};

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// Path to configs
    home_dir: PathBuf,
    /// Test name
    test_name: String,
}

pub const ACC_SENDER: &str = "1b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f";
pub const ACC_RECEIVER: &str = "4d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d0766";

pub const ACC_SENDER_PRIVATE: &str =
    "9cb6b0035320266e430eac9d96745769e7efcf30d2b9cc21ff000b3f873dc2a8";
pub const ACC_RECEIVER_PRIVATE: &str =
    "a55f4f98d2f265c91d8a9868564242d8070b9bf7180a29363f52eb76988636fd";

pub const TIME_TO_WAIT_FOR_BLOCK_SECONDS: u64 = 12;

#[allow(clippy::type_complexity)]
pub async fn pre_test(
    home_dir: PathBuf,
) -> Result<(ServerHandle, JoinHandle<Result<()>>, TempDir)> {
    let home_dir_sequencer = home_dir.join("sequencer");

    let mut sequencer_config =
        sequencer_runner::config::from_file(home_dir_sequencer.join("sequencer_config.json"))
            .unwrap();

    let temp_dir_sequencer = replace_home_dir_with_temp_dir_in_configs(&mut sequencer_config);

    let (seq_http_server_handle, sequencer_loop_handle) =
        startup_sequencer(sequencer_config).await?;

    Ok((
        seq_http_server_handle,
        sequencer_loop_handle,
        temp_dir_sequencer,
    ))
}

pub fn replace_home_dir_with_temp_dir_in_configs(
    sequencer_config: &mut SequencerConfig,
) -> TempDir {
    let temp_dir_sequencer = tempfile::tempdir().unwrap();

    sequencer_config.home = temp_dir_sequencer.path().to_path_buf();

    temp_dir_sequencer
}

#[allow(clippy::type_complexity)]
pub async fn post_test(residual: (ServerHandle, JoinHandle<Result<()>>, TempDir)) {
    let (seq_http_server_handle, sequencer_loop_handle, _) = residual;

    info!("Cleanup");

    sequencer_loop_handle.abort();
    seq_http_server_handle.stop(true).await;

    let wallet_home = wallet::helperfunctions::get_home().unwrap();
    let persistent_data_home = wallet_home.join("curr_accounts.json");

    //Removing persistent accounts after run to not affect other executions
    //Not necessary an error, if fails as there is tests for failure scenario
    let _ = std::fs::remove_file(persistent_data_home)
        .inspect_err(|err| warn!("Failed to remove persistent data with err {err:#?}"));

    //At this point all of the references to sequencer_core must be lost.
    //So they are dropped and tempdirs will be dropped too,
}

pub async fn test_success() {
    info!("test_success");
    let command = Command::SendNativeTokenTransferPublic {
        from: ACC_SENDER.to_string(),
        to: ACC_RECEIVER.to_string(),
        amount: 100,
    };

    let wallet_config = fetch_config().unwrap();

    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

    wallet::execute_subcommand(command).await.unwrap();

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    info!("Checking correct balance move");
    let acc_1_balance = seq_client
        .get_account_balance(ACC_SENDER.to_string())
        .await
        .unwrap();
    let acc_2_balance = seq_client
        .get_account_balance(ACC_RECEIVER.to_string())
        .await
        .unwrap();

    info!("Balance of sender : {acc_1_balance:#?}");
    info!("Balance of receiver : {acc_2_balance:#?}");

    assert_eq!(acc_1_balance.balance, 9900);
    assert_eq!(acc_2_balance.balance, 20100);

    info!("Success!");
}

pub async fn test_success_move_to_another_account() {
    info!("test_success_move_to_another_account");
    let command = Command::RegisterAccountPublic {};

    let wallet_config = fetch_config().unwrap();

    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

    wallet::execute_subcommand(command).await.unwrap();

    let persistent_accounts = fetch_persistent_accounts().unwrap();

    let mut new_persistent_account_addr = String::new();

    for per_acc in persistent_accounts {
        if (per_acc.address().to_string() != ACC_RECEIVER)
            && (per_acc.address().to_string() != ACC_SENDER)
        {
            new_persistent_account_addr = per_acc.address().to_string();
        }
    }

    if new_persistent_account_addr == String::new() {
        panic!("Failed to produce new account, not present in persistent accounts");
    }

    let command = Command::SendNativeTokenTransferPublic {
        from: ACC_SENDER.to_string(),
        to: new_persistent_account_addr.clone(),
        amount: 100,
    };

    wallet::execute_subcommand(command).await.unwrap();

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    info!("Checking correct balance move");
    let acc_1_balance = seq_client
        .get_account_balance(ACC_SENDER.to_string())
        .await
        .unwrap();
    let acc_2_balance = seq_client
        .get_account_balance(new_persistent_account_addr)
        .await
        .unwrap();

    info!("Balance of sender : {acc_1_balance:#?}");
    info!("Balance of receiver : {acc_2_balance:#?}");

    assert_eq!(acc_1_balance.balance, 9900);
    assert_eq!(acc_2_balance.balance, 100);

    info!("Success!");
}

pub async fn test_failure() {
    info!("test_failure");
    let command = Command::SendNativeTokenTransferPublic {
        from: ACC_SENDER.to_string(),
        to: ACC_RECEIVER.to_string(),
        amount: 1000000,
    };

    let wallet_config = fetch_config().unwrap();

    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

    let failed_send = wallet::execute_subcommand(command).await;

    assert!(failed_send.is_err());

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    info!("Checking correct balance move");
    let acc_1_balance = seq_client
        .get_account_balance(ACC_SENDER.to_string())
        .await
        .unwrap();
    let acc_2_balance = seq_client
        .get_account_balance(ACC_RECEIVER.to_string())
        .await
        .unwrap();

    info!("Balance of sender : {acc_1_balance:#?}");
    info!("Balance of receiver : {acc_2_balance:#?}");

    assert_eq!(acc_1_balance.balance, 10000);
    assert_eq!(acc_2_balance.balance, 20000);

    info!("Success!");
}

pub async fn test_success_two_transactions() {
    info!("test_success_two_transactions");
    let command = Command::SendNativeTokenTransferPublic {
        from: ACC_SENDER.to_string(),
        to: ACC_RECEIVER.to_string(),
        amount: 100,
    };

    let wallet_config = fetch_config().unwrap();

    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

    wallet::execute_subcommand(command).await.unwrap();

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    info!("Checking correct balance move");
    let acc_1_balance = seq_client
        .get_account_balance(ACC_SENDER.to_string())
        .await
        .unwrap();
    let acc_2_balance = seq_client
        .get_account_balance(ACC_RECEIVER.to_string())
        .await
        .unwrap();

    info!("Balance of sender : {acc_1_balance:#?}");
    info!("Balance of receiver : {acc_2_balance:#?}");

    assert_eq!(acc_1_balance.balance, 9900);
    assert_eq!(acc_2_balance.balance, 20100);

    info!("First TX Success!");

    let command = Command::SendNativeTokenTransferPublic {
        from: ACC_SENDER.to_string(),
        to: ACC_RECEIVER.to_string(),
        amount: 100,
    };

    wallet::execute_subcommand(command).await.unwrap();

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    info!("Checking correct balance move");
    let acc_1_balance = seq_client
        .get_account_balance(ACC_SENDER.to_string())
        .await
        .unwrap();
    let acc_2_balance = seq_client
        .get_account_balance(ACC_RECEIVER.to_string())
        .await
        .unwrap();

    info!("Balance of sender : {acc_1_balance:#?}");
    info!("Balance of receiver : {acc_2_balance:#?}");

    assert_eq!(acc_1_balance.balance, 9800);
    assert_eq!(acc_2_balance.balance, 20200);

    info!("Second TX Success!");
}

pub async fn test_get_account() {
    info!("test_get_account");
    let wallet_config = fetch_config().unwrap();
    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

    let account = seq_client
        .get_account(ACC_SENDER.to_string())
        .await
        .unwrap()
        .account;

    assert_eq!(
        account.program_owner,
        Program::authenticated_transfer_program().id()
    );
    assert_eq!(account.balance, 10000);
    assert!(account.data.is_empty());
    assert_eq!(account.nonce, 0);
}

/// This test creates a new token using the token program. After creating the token, the test executes a
/// token transfer to a new account.
pub async fn test_success_token_program() {
    let wallet_config = fetch_config().unwrap();

    // Create new account for the token definition
    wallet::execute_subcommand(Command::RegisterAccountPublic {})
        .await
        .unwrap();
    // Create new account for the token supply holder
    wallet::execute_subcommand(Command::RegisterAccountPublic {})
        .await
        .unwrap();
    // Create new account for receiving a token transaction
    wallet::execute_subcommand(Command::RegisterAccountPublic {})
        .await
        .unwrap();

    let persistent_accounts = fetch_persistent_accounts().unwrap();

    let mut new_persistent_accounts_addr = Vec::new();

    for per_acc in persistent_accounts {
        match per_acc {
            PersistentAccountData::Public(per_acc) => {
                if (per_acc.address.to_string() != ACC_RECEIVER)
                    && (per_acc.address.to_string() != ACC_SENDER)
                {
                    new_persistent_accounts_addr.push(per_acc.address);
                }
            }
            _ => continue,
        }
    }

    let [definition_addr, supply_addr, recipient_addr] = new_persistent_accounts_addr
        .try_into()
        .expect("Failed to produce new account, not present in persistent accounts");

    // Create new token
    let command = Command::CreateNewToken {
        definition_addr: definition_addr.to_string(),
        supply_addr: supply_addr.to_string(),
        name: "A NAME".to_string(),
        total_supply: 37,
    };
    wallet::execute_subcommand(command).await.unwrap();
    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

    // Check the status of the token definition account is the expected after the execution
    let definition_acc = seq_client
        .get_account(definition_addr.to_string())
        .await
        .unwrap()
        .account;

    assert_eq!(definition_acc.program_owner, Program::token().id());
    // The data of a token definition account has the following layout:
    // [ 0x00 || name (6 bytes) || total supply (little endian 16 bytes) ]
    assert_eq!(
        definition_acc.data,
        vec![
            0, 65, 32, 78, 65, 77, 69, 37, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
        ]
    );

    // Check the status of the token holding account with the total supply is the expected after the execution
    let supply_acc = seq_client
        .get_account(supply_addr.to_string())
        .await
        .unwrap()
        .account;

    // The account must be owned by the token program
    assert_eq!(supply_acc.program_owner, Program::token().id());
    // The data of a token definition account has the following layout:
    // [ 0x01 || corresponding_token_definition_id (32 bytes) || balance (little endian 16 bytes) ]
    // First byte of the data equal to 1 means it's a token holding account
    assert_eq!(supply_acc.data[0], 1);
    // Bytes from 1 to 33 represent the id of the token this account is associated with.
    // In this example, this is a token account of the newly created token, so it is expected
    // to be equal to the address of the token definition account.
    assert_eq!(
        &supply_acc.data[1..33],
        nssa::AccountId::from(&definition_addr).to_bytes()
    );
    assert_eq!(
        u128::from_le_bytes(supply_acc.data[33..].try_into().unwrap()),
        37
    );

    // Transfer 7 tokens from `supply_acc` to the account at address `recipient_addr`
    let command = Command::TransferToken {
        sender_addr: supply_addr.to_string(),
        recipient_addr: recipient_addr.to_string(),
        balance_to_move: 7,
    };
    wallet::execute_subcommand(command).await.unwrap();
    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    // Check the status of the account at `supply_addr` is the expected after the execution
    let supply_acc = seq_client
        .get_account(supply_addr.to_string())
        .await
        .unwrap()
        .account;
    // The account must be owned by the token program
    assert_eq!(supply_acc.program_owner, Program::token().id());
    // First byte equal to 1 means it's a token holding account
    assert_eq!(supply_acc.data[0], 1);
    // Bytes from 1 to 33 represent the id of the token this account is associated with.
    assert_eq!(
        &supply_acc.data[1..33],
        nssa::AccountId::from(&definition_addr).to_bytes()
    );
    assert_eq!(
        u128::from_le_bytes(supply_acc.data[33..].try_into().unwrap()),
        30
    );

    // Check the status of the account at `recipient_addr` is the expected after the execution
    let recipient_acc = seq_client
        .get_account(recipient_addr.to_string())
        .await
        .unwrap()
        .account;

    // The account must be owned by the token program
    assert_eq!(recipient_acc.program_owner, Program::token().id());
    // First byte equal to 1 means it's a token holding account
    assert_eq!(recipient_acc.data[0], 1);
    // Bytes from 1 to 33 represent the id of the token this account is associated with.
    assert_eq!(
        &recipient_acc.data[1..33],
        nssa::AccountId::from(&definition_addr).to_bytes()
    );
    assert_eq!(
        u128::from_le_bytes(recipient_acc.data[33..].try_into().unwrap()),
        7
    );
}

pub async fn test_success_private_transfer_to_another_owned_account() {
    info!("test_success_private_transfer_to_another_owned_account");
    let command = Command::SendNativeTokenTransferPrivateOwnedAccount {
        from: ACC_SENDER_PRIVATE.to_string(),
        to: ACC_RECEIVER_PRIVATE.to_string(),
        amount: 100,
    };

    let from = produce_account_addr_from_hex(ACC_SENDER_PRIVATE.to_string()).unwrap();
    let to = produce_account_addr_from_hex(ACC_RECEIVER_PRIVATE.to_string()).unwrap();

    let wallet_config = fetch_config().unwrap();

    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

    let mut wallet_storage = WalletCore::start_from_config_update_chain(wallet_config).unwrap();

    wallet::execute_subcommand(command).await.unwrap();

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    let new_commitment1 = {
        let from_acc = wallet_storage
            .storage
            .user_data
            .get_private_account_mut(&from)
            .unwrap();

        from_acc.1.program_owner = nssa::program::Program::authenticated_transfer_program().id();
        from_acc.1.balance -= 100;
        from_acc.1.nonce += 1;

        nssa_core::Commitment::new(&from_acc.0.nullifer_public_key, &from_acc.1)
    };

    let new_commitment2 = {
        let to_acc = wallet_storage
            .storage
            .user_data
            .get_private_account_mut(&to)
            .unwrap();

        to_acc.1.program_owner = nssa::program::Program::authenticated_transfer_program().id();
        to_acc.1.balance += 100;
        to_acc.1.nonce += 1;

        nssa_core::Commitment::new(&to_acc.0.nullifer_public_key, &to_acc.1)
    };

    let proof1 = seq_client
        .get_proof_for_commitment(new_commitment1)
        .await
        .unwrap()
        .unwrap();
    let proof2 = seq_client
        .get_proof_for_commitment(new_commitment2)
        .await
        .unwrap()
        .unwrap();

    println!("New proof is {proof1:#?}");
    println!("New proof is {proof2:#?}");

    info!("Success!");
}

pub async fn test_success_private_transfer_to_another_foreign_account() {
    info!("test_success_private_transfer_to_another_foreign_account");
    let to_npk_orig = NullifierPublicKey([42; 32]);
    let to_npk = hex::encode(to_npk_orig.0);
    let to_ipk = Secp256k1Point::from_scalar(to_npk_orig.0);

    let command = Command::SendNativeTokenTransferPrivateForeignAccount {
        from: ACC_SENDER_PRIVATE.to_string(),
        to_npk,
        to_ipk: hex::encode(to_ipk.0),
        amount: 100,
    };

    let from = produce_account_addr_from_hex(ACC_SENDER_PRIVATE.to_string()).unwrap();

    let wallet_config = fetch_config().unwrap();

    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

    let mut wallet_storage = WalletCore::start_from_config_update_chain(wallet_config).unwrap();

    let sub_ret = wallet::execute_subcommand(command).await.unwrap();

    println!("SUB RET is {sub_ret:#?}");

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    let new_commitment1 = {
        let from_acc = wallet_storage
            .storage
            .user_data
            .get_private_account_mut(&from)
            .unwrap();

        from_acc.1.program_owner = nssa::program::Program::authenticated_transfer_program().id();
        from_acc.1.balance -= 100;
        from_acc.1.nonce += 1;

        nssa_core::Commitment::new(&from_acc.0.nullifer_public_key, &from_acc.1)
    };

    let new_commitment2 = {
        let to_acc = nssa_core::account::Account {
            program_owner: nssa::program::Program::authenticated_transfer_program().id(),
            balance: 100,
            data: vec![],
            nonce: 1,
        };

        nssa_core::Commitment::new(&to_npk_orig, &to_acc)
    };

    let proof1 = seq_client
        .get_proof_for_commitment(new_commitment1)
        .await
        .unwrap()
        .unwrap();
    let proof2 = seq_client
        .get_proof_for_commitment(new_commitment2)
        .await
        .unwrap()
        .unwrap();

    println!("New proof is {proof1:#?}");
    println!("New proof is {proof2:#?}");

    info!("Success!");
}

pub async fn test_success_private_transfer_to_another_owned_account_claiming_path() {
    info!("test_success_private_transfer_to_another_owned_account_claiming_path");
    let command = Command::RegisterAccountPrivate {};

    let sub_ret = wallet::execute_subcommand(command).await.unwrap();

    let SubcommandReturnValue::RegisterAccount { addr: to_addr } = sub_ret else {
        panic!("FAILED TO REGISTER ACCOUNT");
    };

    let wallet_config = fetch_config().unwrap();

    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

    let mut wallet_storage =
        WalletCore::start_from_config_update_chain(wallet_config.clone()).unwrap();

    let (to_keys, mut to_acc) = wallet_storage
        .storage
        .user_data
        .user_private_accounts
        .get(&to_addr)
        .cloned()
        .unwrap();

    let command = Command::SendNativeTokenTransferPrivateForeignAccount {
        from: ACC_SENDER_PRIVATE.to_string(),
        to_npk: hex::encode(to_keys.nullifer_public_key.0),
        to_ipk: hex::encode(to_keys.incoming_viewing_public_key.0),
        amount: 100,
    };

    let from = produce_account_addr_from_hex(ACC_SENDER_PRIVATE.to_string()).unwrap();

    let sub_ret = wallet::execute_subcommand(command).await.unwrap();

    let SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash } = sub_ret else {
        panic!("FAILED TO SEND TX");
    };

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    let new_commitment1 = {
        let from_acc = wallet_storage
            .storage
            .user_data
            .get_private_account_mut(&from)
            .unwrap();

        from_acc.1.program_owner = nssa::program::Program::authenticated_transfer_program().id();
        from_acc.1.balance -= 100;
        from_acc.1.nonce += 1;

        nssa_core::Commitment::new(&from_acc.0.nullifer_public_key, &from_acc.1)
    };

    let new_commitment2 = {
        to_acc.program_owner = nssa::program::Program::authenticated_transfer_program().id();
        to_acc.balance = 100;
        to_acc.nonce = 1;

        nssa_core::Commitment::new(&to_keys.nullifer_public_key, &to_acc)
    };

    let proof1 = seq_client
        .get_proof_for_commitment(new_commitment1)
        .await
        .unwrap()
        .unwrap();
    let proof2 = seq_client
        .get_proof_for_commitment(new_commitment2)
        .await
        .unwrap()
        .unwrap();

    println!("New proof is {proof1:#?}");
    println!("New proof is {proof2:#?}");

    let command = Command::ClaimPrivateAccount {
        tx_hash,
        acc_addr: hex::encode(to_addr),
        ciph_id: 1,
    };

    wallet::execute_subcommand(command).await.unwrap();

    let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config).unwrap();

    let (_, to_res_acc) = wallet_storage
        .storage
        .user_data
        .get_private_account(&to_addr)
        .unwrap();

    assert_eq!(to_res_acc.balance, 100);

    info!("Success!");
}

pub async fn test_success_deshielded_transfer_to_another_account() {
    info!("test_success_deshielded_transfer_to_another_account");
    let command = Command::SendNativeTokenTransferDeshielded {
        from: ACC_SENDER_PRIVATE.to_string(),
        to: ACC_RECEIVER.to_string(),
        amount: 100,
    };

    let from = produce_account_addr_from_hex(ACC_SENDER_PRIVATE.to_string()).unwrap();

    let wallet_config = fetch_config().unwrap();

    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

    let mut wallet_storage = WalletCore::start_from_config_update_chain(wallet_config).unwrap();

    wallet::execute_subcommand(command).await.unwrap();

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    let new_commitment1 = {
        let from_acc = wallet_storage
            .storage
            .user_data
            .get_private_account_mut(&from)
            .unwrap();

        from_acc.1.program_owner = nssa::program::Program::authenticated_transfer_program().id();
        from_acc.1.balance -= 100;
        from_acc.1.nonce += 1;

        nssa_core::Commitment::new(&from_acc.0.nullifer_public_key, &from_acc.1)
    };

    let proof1 = seq_client
        .get_proof_for_commitment(new_commitment1)
        .await
        .unwrap()
        .unwrap();

    let acc_2_balance = seq_client
        .get_account_balance(ACC_RECEIVER.to_string())
        .await
        .unwrap();

    println!("New proof is {proof1:#?}");
    assert_eq!(acc_2_balance.balance, 20100);

    info!("Success!");
}

pub async fn test_success_shielded_transfer_to_another_owned_account() {
    info!("test_success_shielded_transfer_to_another_owned_account");
    let command = Command::SendNativeTokenTransferShielded {
        from: ACC_SENDER.to_string(),
        to: ACC_RECEIVER_PRIVATE.to_string(),
        amount: 100,
    };

    let to = produce_account_addr_from_hex(ACC_RECEIVER_PRIVATE.to_string()).unwrap();

    let wallet_config = fetch_config().unwrap();

    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

    let mut wallet_storage = WalletCore::start_from_config_update_chain(wallet_config).unwrap();

    wallet::execute_subcommand(command).await.unwrap();

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    let new_commitment2 = {
        let to_acc = wallet_storage
            .storage
            .user_data
            .get_private_account_mut(&to)
            .unwrap();

        to_acc.1.program_owner = nssa::program::Program::authenticated_transfer_program().id();
        to_acc.1.balance += 100;
        to_acc.1.nonce += 1;

        nssa_core::Commitment::new(&to_acc.0.nullifer_public_key, &to_acc.1)
    };

    let acc_1_balance = seq_client
        .get_account_balance(ACC_SENDER.to_string())
        .await
        .unwrap();

    let proof2 = seq_client
        .get_proof_for_commitment(new_commitment2)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(acc_1_balance.balance, 9900);

    println!("New proof is {proof2:#?}");

    info!("Success!");
}

pub async fn test_success_shielded_transfer_to_another_foreign_account() {
    info!("test_success_shielded_transfer_to_another_foreign_account");
    let to_npk_orig = NullifierPublicKey([42; 32]);
    let to_npk = hex::encode(to_npk_orig.0);
    let to_ipk = Secp256k1Point::from_scalar(to_npk_orig.0);

    let command = Command::SendNativeTokenTransferShieldedForeignAccount {
        from: ACC_SENDER.to_string(),
        to_npk,
        to_ipk: hex::encode(to_ipk.0),
        amount: 100,
    };

    let wallet_config = fetch_config().unwrap();

    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

    wallet::execute_subcommand(command).await.unwrap();

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    let new_commitment2 = {
        let to_acc = nssa_core::account::Account {
            program_owner: nssa::program::Program::authenticated_transfer_program().id(),
            balance: 100,
            data: vec![],
            nonce: 1,
        };

        nssa_core::Commitment::new(&to_npk_orig, &to_acc)
    };

    let acc_1_balance = seq_client
        .get_account_balance(ACC_SENDER.to_string())
        .await
        .unwrap();

    let proof2 = seq_client
        .get_proof_for_commitment(new_commitment2)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(acc_1_balance.balance, 9900);
    println!("New proof is {proof2:#?}");

    info!("Success!");
}

pub async fn test_success_shielded_transfer_to_another_owned_account_claiming_path() {
    info!("test_success_shielded_transfer_to_another_owned_account_claiming_path");
    let command = Command::RegisterAccountPrivate {};

    let sub_ret = wallet::execute_subcommand(command).await.unwrap();

    let SubcommandReturnValue::RegisterAccount { addr: to_addr } = sub_ret else {
        panic!("FAILED TO REGISTER ACCOUNT");
    };

    let wallet_config = fetch_config().unwrap();

    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

    let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config.clone()).unwrap();

    let (to_keys, mut to_acc) = wallet_storage
        .storage
        .user_data
        .user_private_accounts
        .get(&to_addr)
        .cloned()
        .unwrap();

    let command = Command::SendNativeTokenTransferShieldedForeignAccount {
        from: ACC_SENDER.to_string(),
        to_npk: hex::encode(to_keys.nullifer_public_key.0),
        to_ipk: hex::encode(to_keys.incoming_viewing_public_key.0),
        amount: 100,
    };

    let sub_ret = wallet::execute_subcommand(command).await.unwrap();

    let SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash } = sub_ret else {
        panic!("FAILED TO SEND TX");
    };

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    let new_commitment2 = {
        to_acc.program_owner = nssa::program::Program::authenticated_transfer_program().id();
        to_acc.balance = 100;
        to_acc.nonce = 1;

        nssa_core::Commitment::new(&to_keys.nullifer_public_key, &to_acc)
    };

    let acc_1_balance = seq_client
        .get_account_balance(ACC_SENDER.to_string())
        .await
        .unwrap();

    let proof2 = seq_client
        .get_proof_for_commitment(new_commitment2)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(acc_1_balance.balance, 9900);
    println!("New proof is {proof2:#?}");

    let command = Command::ClaimPrivateAccount {
        tx_hash,
        acc_addr: hex::encode(to_addr),
        ciph_id: 0,
    };

    wallet::execute_subcommand(command).await.unwrap();

    let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config).unwrap();

    let (_, to_res_acc) = wallet_storage
        .storage
        .user_data
        .get_private_account(&to_addr)
        .unwrap();

    assert_eq!(to_res_acc.balance, 100);

    info!("Success!");
}

pub async fn test_pinata() {
    info!("test_pinata");
    let pinata_addr = "cafe".repeat(16);
    let pinata_prize = 150;
    let solution = 989106;
    let command = Command::ClaimPinata {
        pinata_addr: pinata_addr.clone(),
        winner_addr: ACC_SENDER.to_string(),
        solution,
    };

    let wallet_config = fetch_config().unwrap();

    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

    let pinata_balance_pre = seq_client
        .get_account_balance(pinata_addr.clone())
        .await
        .unwrap()
        .balance;

    wallet::execute_subcommand(command).await.unwrap();

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    info!("Checking correct balance move");
    let pinata_balance_post = seq_client
        .get_account_balance(pinata_addr.clone())
        .await
        .unwrap()
        .balance;

    let winner_balance_post = seq_client
        .get_account_balance(ACC_SENDER.to_string())
        .await
        .unwrap()
        .balance;

    assert_eq!(pinata_balance_post, pinata_balance_pre - pinata_prize);
    assert_eq!(winner_balance_post, 10000 + pinata_prize);

    info!("Success!");
}

macro_rules! test_cleanup_wrap {
    ($home_dir:ident, $test_func:ident) => {{
        let res = pre_test($home_dir.clone()).await.unwrap();

        info!("Waiting for first block creation");
        tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

        $test_func().await;

        post_test(res).await;
    }};
}

pub async fn main_tests_runner() -> Result<()> {
    env_logger::init();

    let args = Args::parse();
    let Args {
        home_dir,
        test_name,
    } = args;

    match test_name.as_str() {
        "test_success_token_program" => {
            test_cleanup_wrap!(home_dir, test_success_token_program);
        }
        "test_success_move_to_another_account" => {
            test_cleanup_wrap!(home_dir, test_success_move_to_another_account);
        }
        "test_success" => {
            test_cleanup_wrap!(home_dir, test_success);
        }
        "test_failure" => {
            test_cleanup_wrap!(home_dir, test_failure);
        }
        "test_get_account" => {
            test_cleanup_wrap!(home_dir, test_get_account);
        }
        "test_success_two_transactions" => {
            test_cleanup_wrap!(home_dir, test_success_two_transactions);
        }
        "test_success_private_transfer_to_another_owned_account" => {
            test_cleanup_wrap!(
                home_dir,
                test_success_private_transfer_to_another_owned_account
            );
        }
        "test_success_private_transfer_to_another_foreign_account" => {
            test_cleanup_wrap!(
                home_dir,
                test_success_private_transfer_to_another_foreign_account
            );
        }
        "test_success_private_transfer_to_another_owned_account_claiming_path" => {
            test_cleanup_wrap!(
                home_dir,
                test_success_private_transfer_to_another_owned_account_claiming_path
            );
        }
        "test_success_deshielded_transfer_to_another_account" => {
            test_cleanup_wrap!(
                home_dir,
                test_success_deshielded_transfer_to_another_account
            );
        }
        "test_success_shielded_transfer_to_another_owned_account" => {
            test_cleanup_wrap!(
                home_dir,
                test_success_shielded_transfer_to_another_owned_account
            );
        }
        "test_success_shielded_transfer_to_another_foreign_account" => {
            test_cleanup_wrap!(
                home_dir,
                test_success_shielded_transfer_to_another_foreign_account
            );
        }
        "test_success_shielded_transfer_to_another_owned_account_claiming_path" => {
            test_cleanup_wrap!(
                home_dir,
                test_success_shielded_transfer_to_another_owned_account_claiming_path
            );
        }
        "test_pinata" => {
            test_cleanup_wrap!(home_dir, test_pinata);
        }
        "all" => {
            test_cleanup_wrap!(home_dir, test_success_move_to_another_account);
            test_cleanup_wrap!(home_dir, test_success);
            test_cleanup_wrap!(home_dir, test_failure);
            test_cleanup_wrap!(home_dir, test_success_two_transactions);
            test_cleanup_wrap!(home_dir, test_success_token_program);
            test_cleanup_wrap!(
                home_dir,
                test_success_private_transfer_to_another_owned_account
            );
            test_cleanup_wrap!(
                home_dir,
                test_success_private_transfer_to_another_foreign_account
            );
            test_cleanup_wrap!(
                home_dir,
                test_success_deshielded_transfer_to_another_account
            );
            test_cleanup_wrap!(
                home_dir,
                test_success_shielded_transfer_to_another_owned_account
            );
            test_cleanup_wrap!(
                home_dir,
                test_success_shielded_transfer_to_another_foreign_account
            );
            test_cleanup_wrap!(
                home_dir,
                test_success_private_transfer_to_another_owned_account_claiming_path
            );
            test_cleanup_wrap!(
                home_dir,
                test_success_shielded_transfer_to_another_owned_account_claiming_path
            );
            test_cleanup_wrap!(home_dir, test_pinata);
        }
        _ => {
            anyhow::bail!("Unknown test name");
        }
    }

    Ok(())
}

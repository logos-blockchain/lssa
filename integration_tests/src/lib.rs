use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use std::{path::PathBuf, time::Duration};

use actix_web::dev::ServerHandle;
use anyhow::Result;
use clap::Parser;
use common::{
    sequencer_client::SequencerClient,
    transaction::{EncodedTransaction, NSSATransaction},
};
use log::{info, warn};
use nssa::{Address, PrivacyPreservingTransaction, ProgramDeploymentTransaction, program::Program};
use nssa_core::{
    Commitment, NullifierPublicKey, encryption::shared_key_derivation::Secp256k1Point,
};
use sequencer_core::config::SequencerConfig;
use sequencer_runner::startup_sequencer;
use tempfile::TempDir;
use tokio::task::JoinHandle;
use wallet::{
    Command, SubcommandReturnValue, WalletCore,
    cli::{
        account::{AccountSubcommand, FetchSubcommand, RegisterSubcommand},
        native_token_transfer_program::{
            NativeTokenTransferProgramSubcommand, NativeTokenTransferProgramSubcommandPrivate,
            NativeTokenTransferProgramSubcommandShielded,
        },
        pinata_program::{
            PinataProgramSubcommand, PinataProgramSubcommandPrivate, PinataProgramSubcommandPublic,
        },
        token_program::{
            TokenProgramSubcommand, TokenProgramSubcommandDeshielded,
            TokenProgramSubcommandPrivate, TokenProgramSubcommandPublic,
            TokenProgramSubcommandShielded,
        },
    },
    config::PersistentAccountData,
    helperfunctions::{fetch_config, fetch_persistent_accounts},
};

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// Path to configs
    home_dir: PathBuf,
    /// Test name
    test_name: String,
}

pub const ACC_SENDER: &str = "d07ad2e84b27fa00c262f0a1eea0ff35ca0973547e6a106f72f193c2dc838b44";
pub const ACC_RECEIVER: &str = "e7ae77c5ef1a05999344af499fc78a1705398d62ed06cf2e1479f6def89a39bc";

pub const ACC_SENDER_PRIVATE: &str =
    "d360d6b5763f71ac6af56253687fd7d556d5c6c64312e53c0b92ef039a4375df";
pub const ACC_RECEIVER_PRIVATE: &str =
    "f27087ffc29b99035303697dcf6c8e323b1847d4261e6afd49e0d71c6dfa31ea";

pub const TIME_TO_WAIT_FOR_BLOCK_SECONDS: u64 = 12;

pub const NSSA_PROGRAM_FOR_TEST_DATA_CHANGER: &[u8] = include_bytes!("data_changer.bin");

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
    let command = Command::Transfer(NativeTokenTransferProgramSubcommand::Public {
        from: ACC_SENDER.to_string(),
        to: ACC_RECEIVER.to_string(),
        amount: 100,
    });

    let wallet_config = fetch_config().await.unwrap();

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
    let command = Command::Account(AccountSubcommand::Register(RegisterSubcommand::Public {}));

    let wallet_config = fetch_config().await.unwrap();

    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

    wallet::execute_subcommand(command).await.unwrap();

    let persistent_accounts = fetch_persistent_accounts().await.unwrap();

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

    let command = Command::Transfer(NativeTokenTransferProgramSubcommand::Public {
        from: ACC_SENDER.to_string(),
        to: new_persistent_account_addr.clone(),
        amount: 100,
    });

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
    let command = Command::Transfer(NativeTokenTransferProgramSubcommand::Public {
        from: ACC_SENDER.to_string(),
        to: ACC_RECEIVER.to_string(),
        amount: 1000000,
    });

    let wallet_config = fetch_config().await.unwrap();

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
    let command = Command::Transfer(NativeTokenTransferProgramSubcommand::Public {
        from: ACC_SENDER.to_string(),
        to: ACC_RECEIVER.to_string(),
        amount: 100,
    });

    let wallet_config = fetch_config().await.unwrap();

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

    let command = Command::Transfer(NativeTokenTransferProgramSubcommand::Public {
        from: ACC_SENDER.to_string(),
        to: ACC_RECEIVER.to_string(),
        amount: 100,
    });

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
    let wallet_config = fetch_config().await.unwrap();
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
    let wallet_config = fetch_config().await.unwrap();

    // Create new account for the token definition
    wallet::execute_subcommand(Command::Account(AccountSubcommand::Register(
        RegisterSubcommand::Public {},
    )))
    .await
    .unwrap();
    // Create new account for the token supply holder
    wallet::execute_subcommand(Command::Account(AccountSubcommand::Register(
        RegisterSubcommand::Public {},
    )))
    .await
    .unwrap();
    // Create new account for receiving a token transaction
    wallet::execute_subcommand(Command::Account(AccountSubcommand::Register(
        RegisterSubcommand::Public {},
    )))
    .await
    .unwrap();

    let persistent_accounts = fetch_persistent_accounts().await.unwrap();

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
    let subcommand = TokenProgramSubcommand::Public(TokenProgramSubcommandPublic::CreateNewToken {
        definition_addr: definition_addr.to_string(),
        supply_addr: supply_addr.to_string(),
        name: "A NAME".to_string(),
        total_supply: 37,
    });
    wallet::execute_subcommand(Command::TokenProgram(subcommand))
        .await
        .unwrap();
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
    assert_eq!(&supply_acc.data[1..33], definition_addr.to_bytes());
    assert_eq!(
        u128::from_le_bytes(supply_acc.data[33..].try_into().unwrap()),
        37
    );

    // Transfer 7 tokens from `supply_acc` to the account at address `recipient_addr`
    let subcommand = TokenProgramSubcommand::Public(TokenProgramSubcommandPublic::TransferToken {
        sender_addr: supply_addr.to_string(),
        recipient_addr: recipient_addr.to_string(),
        balance_to_move: 7,
    });
    wallet::execute_subcommand(Command::TokenProgram(subcommand))
        .await
        .unwrap();
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
    assert_eq!(&supply_acc.data[1..33], definition_addr.to_bytes());
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
    assert_eq!(&recipient_acc.data[1..33], definition_addr.to_bytes());
    assert_eq!(
        u128::from_le_bytes(recipient_acc.data[33..].try_into().unwrap()),
        7
    );
}

/// This test creates a new private token using the token program. After creating the token, the test executes a
/// private token transfer to a new account. All accounts are owned except definition.
pub async fn test_success_token_program_private_owned() {
    let wallet_config = fetch_config().await.unwrap();

    // Create new account for the token definition (public)
    let SubcommandReturnValue::RegisterAccount {
        addr: definition_addr,
    } = wallet::execute_subcommand(Command::Account(AccountSubcommand::Register(
        RegisterSubcommand::Public {},
    )))
    .await
    .unwrap()
    else {
        panic!("invalid subcommand return value");
    };
    // Create new account for the token supply holder (private)
    let SubcommandReturnValue::RegisterAccount { addr: supply_addr } = wallet::execute_subcommand(
        Command::Account(AccountSubcommand::Register(RegisterSubcommand::Private {})),
    )
    .await
    .unwrap() else {
        panic!("invalid subcommand return value");
    };
    // Create new account for receiving a token transaction
    let SubcommandReturnValue::RegisterAccount {
        addr: recipient_addr,
    } = wallet::execute_subcommand(Command::Account(AccountSubcommand::Register(
        RegisterSubcommand::Private {},
    )))
    .await
    .unwrap()
    else {
        panic!("invalid subcommand return value");
    };

    // Create new token
    let subcommand = TokenProgramSubcommand::Private(
        TokenProgramSubcommandPrivate::CreateNewTokenPrivateOwned {
            definition_addr: definition_addr.to_string(),
            supply_addr: supply_addr.to_string(),
            name: "A NAME".to_string(),
            total_supply: 37,
        },
    );

    wallet::execute_subcommand(Command::TokenProgram(subcommand))
        .await
        .unwrap();

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

    let wallet_config = fetch_config().await.unwrap();
    let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
        .await
        .unwrap();

    let new_commitment1 = wallet_storage
        .get_private_account_commitment(&supply_addr)
        .unwrap();
    assert!(verify_commitment_is_in_state(new_commitment1, &seq_client).await);

    // Transfer 7 tokens from `supply_acc` to the account at address `recipient_addr`
    let subcommand =
        TokenProgramSubcommand::Private(TokenProgramSubcommandPrivate::TransferTokenPrivateOwned {
            sender_addr: supply_addr.to_string(),
            recipient_addr: recipient_addr.to_string(),
            balance_to_move: 7,
        });

    wallet::execute_subcommand(Command::TokenProgram(subcommand))
        .await
        .unwrap();

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    let wallet_config = fetch_config().await.unwrap();
    let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
        .await
        .unwrap();

    let new_commitment1 = wallet_storage
        .get_private_account_commitment(&supply_addr)
        .unwrap();
    assert!(verify_commitment_is_in_state(new_commitment1, &seq_client).await);

    let new_commitment2 = wallet_storage
        .get_private_account_commitment(&recipient_addr)
        .unwrap();
    assert!(verify_commitment_is_in_state(new_commitment2, &seq_client).await);

    // Transfer additional 7 tokens from `supply_acc` to the account at address `recipient_addr`
    let subcommand =
        TokenProgramSubcommand::Private(TokenProgramSubcommandPrivate::TransferTokenPrivateOwned {
            sender_addr: supply_addr.to_string(),
            recipient_addr: recipient_addr.to_string(),
            balance_to_move: 7,
        });

    wallet::execute_subcommand(Command::TokenProgram(subcommand))
        .await
        .unwrap();

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    let wallet_config = fetch_config().await.unwrap();
    let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
        .await
        .unwrap();

    let new_commitment1 = wallet_storage
        .get_private_account_commitment(&supply_addr)
        .unwrap();
    assert!(verify_commitment_is_in_state(new_commitment1, &seq_client).await);

    let new_commitment2 = wallet_storage
        .get_private_account_commitment(&recipient_addr)
        .unwrap();
    assert!(verify_commitment_is_in_state(new_commitment2, &seq_client).await);
}

/// This test creates a new private token using the token program. After creating the token, the test executes a
/// private token transfer to a new account.
pub async fn test_success_token_program_private_claiming_path() {
    let wallet_config = fetch_config().await.unwrap();

    // Create new account for the token definition (public)
    let SubcommandReturnValue::RegisterAccount {
        addr: definition_addr,
    } = wallet::execute_subcommand(Command::Account(AccountSubcommand::Register(
        RegisterSubcommand::Public {},
    )))
    .await
    .unwrap()
    else {
        panic!("invalid subcommand return value");
    };
    // Create new account for the token supply holder (private)
    let SubcommandReturnValue::RegisterAccount { addr: supply_addr } = wallet::execute_subcommand(
        Command::Account(AccountSubcommand::Register(RegisterSubcommand::Private {})),
    )
    .await
    .unwrap() else {
        panic!("invalid subcommand return value");
    };
    // Create new account for receiving a token transaction
    let SubcommandReturnValue::RegisterAccount {
        addr: recipient_addr,
    } = wallet::execute_subcommand(Command::Account(AccountSubcommand::Register(
        RegisterSubcommand::Private {},
    )))
    .await
    .unwrap()
    else {
        panic!("invalid subcommand return value");
    };

    // Create new token
    let subcommand = TokenProgramSubcommand::Private(
        TokenProgramSubcommandPrivate::CreateNewTokenPrivateOwned {
            definition_addr: definition_addr.to_string(),
            supply_addr: supply_addr.to_string(),
            name: "A NAME".to_string(),
            total_supply: 37,
        },
    );

    wallet::execute_subcommand(Command::TokenProgram(subcommand))
        .await
        .unwrap();

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

    let wallet_config = fetch_config().await.unwrap();
    let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
        .await
        .unwrap();

    let new_commitment1 = wallet_storage
        .get_private_account_commitment(&supply_addr)
        .unwrap();
    assert!(verify_commitment_is_in_state(new_commitment1, &seq_client).await);

    let (recipient_keys, _) = wallet_storage
        .storage
        .user_data
        .get_private_account(&recipient_addr)
        .unwrap();

    // Transfer 7 tokens from `supply_acc` to the account at address `recipient_addr`
    let subcommand = TokenProgramSubcommand::Private(
        TokenProgramSubcommandPrivate::TransferTokenPrivateForeign {
            sender_addr: supply_addr.to_string(),
            recipient_npk: hex::encode(recipient_keys.nullifer_public_key.0),
            recipient_ipk: hex::encode(recipient_keys.incoming_viewing_public_key.0.clone()),
            balance_to_move: 7,
        },
    );

    let SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash } =
        wallet::execute_subcommand(Command::TokenProgram(subcommand))
            .await
            .unwrap()
    else {
        panic!("invalid subcommand return value");
    };

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    let command = Command::Account(AccountSubcommand::Fetch(FetchSubcommand::PrivateAccount {
        tx_hash,
        acc_addr: recipient_addr.to_string(),
        output_id: 1,
    }));

    wallet::execute_subcommand(command).await.unwrap();

    let wallet_config = fetch_config().await.unwrap();
    let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
        .await
        .unwrap();

    let new_commitment1 = wallet_storage
        .get_private_account_commitment(&supply_addr)
        .unwrap();
    assert!(verify_commitment_is_in_state(new_commitment1, &seq_client).await);

    let new_commitment2 = wallet_storage
        .get_private_account_commitment(&recipient_addr)
        .unwrap();
    assert!(verify_commitment_is_in_state(new_commitment2, &seq_client).await);
}

/// This test creates a new public token using the token program. After creating the token, the test executes a
/// shielded token transfer to a new account. All accounts are owned except definition.
pub async fn test_success_token_program_shielded_owned() {
    let wallet_config = fetch_config().await.unwrap();

    // Create new account for the token definition (public)
    let SubcommandReturnValue::RegisterAccount {
        addr: definition_addr,
    } = wallet::execute_subcommand(Command::Account(AccountSubcommand::Register(
        RegisterSubcommand::Public {},
    )))
    .await
    .unwrap()
    else {
        panic!("invalid subcommand return value");
    };
    // Create new account for the token supply holder (public)
    let SubcommandReturnValue::RegisterAccount { addr: supply_addr } = wallet::execute_subcommand(
        Command::Account(AccountSubcommand::Register(RegisterSubcommand::Public {})),
    )
    .await
    .unwrap() else {
        panic!("invalid subcommand return value");
    };
    // Create new account for receiving a token transaction
    let SubcommandReturnValue::RegisterAccount {
        addr: recipient_addr,
    } = wallet::execute_subcommand(Command::Account(AccountSubcommand::Register(
        RegisterSubcommand::Private {},
    )))
    .await
    .unwrap()
    else {
        panic!("invalid subcommand return value");
    };

    // Create new token
    let subcommand = TokenProgramSubcommand::Public(TokenProgramSubcommandPublic::CreateNewToken {
        definition_addr: definition_addr.to_string(),
        supply_addr: supply_addr.to_string(),
        name: "A NAME".to_string(),
        total_supply: 37,
    });

    wallet::execute_subcommand(Command::TokenProgram(subcommand))
        .await
        .unwrap();

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

    // Transfer 7 tokens from `supply_acc` to the account at address `recipient_addr`
    let subcommand = TokenProgramSubcommand::Shielded(
        TokenProgramSubcommandShielded::TransferTokenShieldedOwned {
            sender_addr: supply_addr.to_string(),
            recipient_addr: recipient_addr.to_string(),
            balance_to_move: 7,
        },
    );

    wallet::execute_subcommand(Command::TokenProgram(subcommand))
        .await
        .unwrap();

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    let wallet_config = fetch_config().await.unwrap();
    let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
        .await
        .unwrap();

    let new_commitment2 = wallet_storage
        .get_private_account_commitment(&recipient_addr)
        .unwrap();
    assert!(verify_commitment_is_in_state(new_commitment2, &seq_client).await);

    // Transfer additional 7 tokens from `supply_acc` to the account at address `recipient_addr`
    let subcommand = TokenProgramSubcommand::Shielded(
        TokenProgramSubcommandShielded::TransferTokenShieldedOwned {
            sender_addr: supply_addr.to_string(),
            recipient_addr: recipient_addr.to_string(),
            balance_to_move: 7,
        },
    );

    wallet::execute_subcommand(Command::TokenProgram(subcommand))
        .await
        .unwrap();

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    let wallet_config = fetch_config().await.unwrap();
    let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
        .await
        .unwrap();

    let new_commitment2 = wallet_storage
        .get_private_account_commitment(&recipient_addr)
        .unwrap();
    assert!(verify_commitment_is_in_state(new_commitment2, &seq_client).await);
}

/// This test creates a new private token using the token program. After creating the token, the test executes a
/// deshielded token transfer to a new account. All accounts are owned except definition.
pub async fn test_success_token_program_deshielded_owned() {
    let wallet_config = fetch_config().await.unwrap();

    // Create new account for the token definition (public)
    let SubcommandReturnValue::RegisterAccount {
        addr: definition_addr,
    } = wallet::execute_subcommand(Command::Account(AccountSubcommand::Register(
        RegisterSubcommand::Public {},
    )))
    .await
    .unwrap()
    else {
        panic!("invalid subcommand return value");
    };
    // Create new account for the token supply holder (private)
    let SubcommandReturnValue::RegisterAccount { addr: supply_addr } = wallet::execute_subcommand(
        Command::Account(AccountSubcommand::Register(RegisterSubcommand::Private {})),
    )
    .await
    .unwrap() else {
        panic!("invalid subcommand return value");
    };
    // Create new account for receiving a token transaction
    let SubcommandReturnValue::RegisterAccount {
        addr: recipient_addr,
    } = wallet::execute_subcommand(Command::Account(AccountSubcommand::Register(
        RegisterSubcommand::Public {},
    )))
    .await
    .unwrap()
    else {
        panic!("invalid subcommand return value");
    };

    // Create new token
    let subcommand = TokenProgramSubcommand::Private(
        TokenProgramSubcommandPrivate::CreateNewTokenPrivateOwned {
            definition_addr: definition_addr.to_string(),
            supply_addr: supply_addr.to_string(),
            name: "A NAME".to_string(),
            total_supply: 37,
        },
    );

    wallet::execute_subcommand(Command::TokenProgram(subcommand))
        .await
        .unwrap();

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

    let wallet_config = fetch_config().await.unwrap();
    let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
        .await
        .unwrap();

    let new_commitment1 = wallet_storage
        .get_private_account_commitment(&supply_addr)
        .unwrap();
    assert!(verify_commitment_is_in_state(new_commitment1, &seq_client).await);

    // Transfer 7 tokens from `supply_acc` to the account at address `recipient_addr`
    let subcommand = TokenProgramSubcommand::Deshielded(
        TokenProgramSubcommandDeshielded::TransferTokenDeshielded {
            sender_addr: supply_addr.to_string(),
            recipient_addr: recipient_addr.to_string(),
            balance_to_move: 7,
        },
    );

    wallet::execute_subcommand(Command::TokenProgram(subcommand))
        .await
        .unwrap();

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    let wallet_config = fetch_config().await.unwrap();
    let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
        .await
        .unwrap();

    let new_commitment1 = wallet_storage
        .get_private_account_commitment(&supply_addr)
        .unwrap();
    assert!(verify_commitment_is_in_state(new_commitment1, &seq_client).await);

    // Transfer additional 7 tokens from `supply_acc` to the account at address `recipient_addr`
    let subcommand = TokenProgramSubcommand::Deshielded(
        TokenProgramSubcommandDeshielded::TransferTokenDeshielded {
            sender_addr: supply_addr.to_string(),
            recipient_addr: recipient_addr.to_string(),
            balance_to_move: 7,
        },
    );

    wallet::execute_subcommand(Command::TokenProgram(subcommand))
        .await
        .unwrap();

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    let wallet_config = fetch_config().await.unwrap();
    let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
        .await
        .unwrap();

    let new_commitment1 = wallet_storage
        .get_private_account_commitment(&supply_addr)
        .unwrap();
    assert!(verify_commitment_is_in_state(new_commitment1, &seq_client).await);
}

pub async fn test_success_private_transfer_to_another_owned_account() {
    info!("test_success_private_transfer_to_another_owned_account");
    let from: Address = ACC_SENDER_PRIVATE.parse().unwrap();
    let to: Address = ACC_RECEIVER_PRIVATE.parse().unwrap();

    let command = Command::Transfer(NativeTokenTransferProgramSubcommand::Private(
        NativeTokenTransferProgramSubcommandPrivate::PrivateOwned {
            from: from.to_string(),
            to: to.to_string(),
            amount: 100,
        },
    ));

    wallet::execute_subcommand(command).await.unwrap();

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    let wallet_config = fetch_config().await.unwrap();
    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();
    let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
        .await
        .unwrap();

    let new_commitment1 = wallet_storage
        .get_private_account_commitment(&from)
        .unwrap();
    assert!(verify_commitment_is_in_state(new_commitment1, &seq_client).await);

    let new_commitment2 = wallet_storage.get_private_account_commitment(&to).unwrap();
    assert!(verify_commitment_is_in_state(new_commitment2, &seq_client).await);

    info!("Success!");
}

pub async fn test_success_private_transfer_to_another_foreign_account() {
    info!("test_success_private_transfer_to_another_foreign_account");
    let from: Address = ACC_SENDER_PRIVATE.parse().unwrap();
    let to_npk = NullifierPublicKey([42; 32]);
    let to_npk_string = hex::encode(to_npk.0);
    let to_ipk = Secp256k1Point::from_scalar(to_npk.0);

    let command = Command::Transfer(NativeTokenTransferProgramSubcommand::Private(
        NativeTokenTransferProgramSubcommandPrivate::PrivateForeign {
            from: from.to_string(),
            to_npk: to_npk_string,
            to_ipk: hex::encode(to_ipk.0),
            amount: 100,
        },
    ));

    let SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash } =
        wallet::execute_subcommand(command).await.unwrap()
    else {
        panic!("invalid subcommand return value");
    };

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    let wallet_config = fetch_config().await.unwrap();
    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();
    let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
        .await
        .unwrap();

    let new_commitment1 = wallet_storage
        .get_private_account_commitment(&from)
        .unwrap();

    let tx = fetch_privacy_preserving_tx(&seq_client, tx_hash.clone()).await;
    assert_eq!(tx.message.new_commitments[0], new_commitment1);

    assert_eq!(tx.message.new_commitments.len(), 2);
    for commitment in tx.message.new_commitments.into_iter() {
        assert!(verify_commitment_is_in_state(commitment, &seq_client).await);
    }

    info!("Success!");
}

pub async fn test_success_private_transfer_to_another_owned_account_claiming_path() {
    info!("test_success_private_transfer_to_another_owned_account_claiming_path");
    let from: Address = ACC_SENDER_PRIVATE.parse().unwrap();

    let command = Command::Account(AccountSubcommand::Register(RegisterSubcommand::Private {}));

    let sub_ret = wallet::execute_subcommand(command).await.unwrap();
    let SubcommandReturnValue::RegisterAccount { addr: to_addr } = sub_ret else {
        panic!("FAILED TO REGISTER ACCOUNT");
    };

    let wallet_config = fetch_config().await.unwrap();
    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();
    let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config.clone())
        .await
        .unwrap();

    let (to_keys, _) = wallet_storage
        .storage
        .user_data
        .user_private_accounts
        .get(&to_addr)
        .cloned()
        .unwrap();

    let command = Command::Transfer(NativeTokenTransferProgramSubcommand::Private(
        NativeTokenTransferProgramSubcommandPrivate::PrivateForeign {
            from: from.to_string(),
            to_npk: hex::encode(to_keys.nullifer_public_key.0),
            to_ipk: hex::encode(to_keys.incoming_viewing_public_key.0),
            amount: 100,
        },
    ));

    let sub_ret = wallet::execute_subcommand(command).await.unwrap();
    let SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash } = sub_ret else {
        panic!("FAILED TO SEND TX");
    };

    let tx = fetch_privacy_preserving_tx(&seq_client, tx_hash.clone()).await;

    let command = Command::Account(AccountSubcommand::Fetch(FetchSubcommand::PrivateAccount {
        tx_hash,
        acc_addr: to_addr.to_string(),
        output_id: 1,
    }));
    wallet::execute_subcommand(command).await.unwrap();
    let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
        .await
        .unwrap();

    let new_commitment1 = wallet_storage
        .get_private_account_commitment(&from)
        .unwrap();
    assert_eq!(tx.message.new_commitments[0], new_commitment1);

    assert_eq!(tx.message.new_commitments.len(), 2);
    for commitment in tx.message.new_commitments.into_iter() {
        assert!(verify_commitment_is_in_state(commitment, &seq_client).await);
    }

    let to_res_acc = wallet_storage.get_account_private(&to_addr).unwrap();

    assert_eq!(to_res_acc.balance, 100);

    info!("Success!");
}

pub async fn test_success_private_transfer_to_another_owned_account_cont_run_path() {
    info!("test_success_private_transfer_to_another_owned_account_cont_run_path");
    let continious_run_handle = tokio::spawn(wallet::execute_continious_run());

    let from: Address = ACC_SENDER_PRIVATE.parse().unwrap();

    let command = Command::Account(AccountSubcommand::Register(RegisterSubcommand::Private {}));

    let sub_ret = wallet::execute_subcommand(command).await.unwrap();
    let SubcommandReturnValue::RegisterAccount { addr: to_addr } = sub_ret else {
        panic!("FAILED TO REGISTER ACCOUNT");
    };

    let wallet_config = fetch_config().await.unwrap();
    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();
    let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config.clone())
        .await
        .unwrap();

    let (to_keys, _) = wallet_storage
        .storage
        .user_data
        .user_private_accounts
        .get(&to_addr)
        .cloned()
        .unwrap();

    let command = Command::Transfer(NativeTokenTransferProgramSubcommand::Private(
        NativeTokenTransferProgramSubcommandPrivate::PrivateForeign {
            from: from.to_string(),
            to_npk: hex::encode(to_keys.nullifer_public_key.0),
            to_ipk: hex::encode(to_keys.incoming_viewing_public_key.0),
            amount: 100,
        },
    ));

    let sub_ret = wallet::execute_subcommand(command).await.unwrap();
    let SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash } = sub_ret else {
        panic!("FAILED TO SEND TX");
    };

    let tx = fetch_privacy_preserving_tx(&seq_client, tx_hash.clone()).await;

    println!("Waiting for next blocks to check if continoius run fetch account");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
        .await
        .unwrap();

    let new_commitment1 = wallet_storage
        .get_private_account_commitment(&from)
        .unwrap();
    assert_eq!(tx.message.new_commitments[0], new_commitment1);

    assert_eq!(tx.message.new_commitments.len(), 2);
    for commitment in tx.message.new_commitments.into_iter() {
        assert!(verify_commitment_is_in_state(commitment, &seq_client).await);
    }

    let to_res_acc = wallet_storage.get_account_private(&to_addr).unwrap();

    assert_eq!(to_res_acc.balance, 100);

    continious_run_handle.abort();

    info!("Success!");
}

pub async fn test_success_deshielded_transfer_to_another_account() {
    info!("test_success_deshielded_transfer_to_another_account");
    let from: Address = ACC_SENDER_PRIVATE.parse().unwrap();
    let to: Address = ACC_RECEIVER.parse().unwrap();
    let command = Command::Transfer(NativeTokenTransferProgramSubcommand::Deshielded {
        from: from.to_string(),
        to: to.to_string(),
        amount: 100,
    });

    let wallet_config = fetch_config().await.unwrap();
    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();
    let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config.clone())
        .await
        .unwrap();

    let from_acc = wallet_storage.get_account_private(&from).unwrap();
    assert_eq!(from_acc.balance, 10000);

    wallet::execute_subcommand(command).await.unwrap();

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
        .await
        .unwrap();

    let from_acc = wallet_storage.get_account_private(&from).unwrap();
    let new_commitment = wallet_storage
        .get_private_account_commitment(&from)
        .unwrap();
    assert!(verify_commitment_is_in_state(new_commitment, &seq_client).await);

    let acc_2_balance = seq_client
        .get_account_balance(to.to_string())
        .await
        .unwrap();

    assert_eq!(from_acc.balance, 10000 - 100);
    assert_eq!(acc_2_balance.balance, 20100);

    info!("Success!");
}

pub async fn test_success_shielded_transfer_to_another_owned_account() {
    info!("test_success_shielded_transfer_to_another_owned_account");
    let from: Address = ACC_SENDER.parse().unwrap();
    let to: Address = ACC_RECEIVER_PRIVATE.parse().unwrap();
    let command = Command::Transfer(NativeTokenTransferProgramSubcommand::Shielded(
        NativeTokenTransferProgramSubcommandShielded::ShieldedOwned {
            from: from.to_string(),
            to: to.to_string(),
            amount: 100,
        },
    ));

    let wallet_config = fetch_config().await.unwrap();
    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

    wallet::execute_subcommand(command).await.unwrap();

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    let wallet_config = fetch_config().await.unwrap();
    let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
        .await
        .unwrap();

    let acc_to = wallet_storage.get_account_private(&to).unwrap();
    let new_commitment = wallet_storage.get_private_account_commitment(&to).unwrap();
    assert!(verify_commitment_is_in_state(new_commitment, &seq_client).await);

    let acc_from_balance = seq_client
        .get_account_balance(from.to_string())
        .await
        .unwrap();

    assert_eq!(acc_from_balance.balance, 9900);
    assert_eq!(acc_to.balance, 20000 + 100);

    info!("Success!");
}

pub async fn test_success_shielded_transfer_to_another_foreign_account() {
    info!("test_success_shielded_transfer_to_another_foreign_account");
    let to_npk = NullifierPublicKey([42; 32]);
    let to_npk_string = hex::encode(to_npk.0);
    let to_ipk = Secp256k1Point::from_scalar(to_npk.0);
    let from: Address = ACC_SENDER.parse().unwrap();

    let command = Command::Transfer(NativeTokenTransferProgramSubcommand::Shielded(
        NativeTokenTransferProgramSubcommandShielded::ShieldedForeign {
            from: from.to_string(),
            to_npk: to_npk_string,
            to_ipk: hex::encode(to_ipk.0),
            amount: 100,
        },
    ));

    let wallet_config = fetch_config().await.unwrap();

    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

    let SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash } =
        wallet::execute_subcommand(command).await.unwrap()
    else {
        panic!("invalid subcommand return value");
    };

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    let tx = fetch_privacy_preserving_tx(&seq_client, tx_hash).await;

    let acc_1_balance = seq_client
        .get_account_balance(from.to_string())
        .await
        .unwrap();

    assert!(
        verify_commitment_is_in_state(tx.message.new_commitments[0].clone(), &seq_client).await
    );

    assert_eq!(acc_1_balance.balance, 9900);

    info!("Success!");
}

pub async fn test_pinata() {
    info!("test_pinata");
    let pinata_addr = "cafe".repeat(16);
    let pinata_prize = 150;
    let solution = 989106;
    let command = Command::PinataProgram(PinataProgramSubcommand::Public(
        PinataProgramSubcommandPublic::Claim {
            pinata_addr: pinata_addr.clone(),
            winner_addr: ACC_SENDER.to_string(),
            solution,
        },
    ));

    let wallet_config = fetch_config().await.unwrap();

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

pub async fn test_program_deployment() {
    info!("test program deployment");
    let bytecode = NSSA_PROGRAM_FOR_TEST_DATA_CHANGER.to_vec();
    let message = nssa::program_deployment_transaction::Message::new(bytecode.clone());
    let transaction = ProgramDeploymentTransaction::new(message);

    let wallet_config = fetch_config().await.unwrap();
    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

    let _response = seq_client.send_tx_program(transaction).await.unwrap();

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    // The program is the data changer and takes one account as input.
    // We pass an uninitialized account and we expect after execution to be owned by the data
    // changer program (NSSA account claiming mechanism) with data equal to [0] (due to program logic)
    let data_changer = Program::new(bytecode).unwrap();
    let address: Address = "deadbeef".repeat(8).parse().unwrap();
    let message =
        nssa::public_transaction::Message::try_new(data_changer.id(), vec![address], vec![], ())
            .unwrap();
    let witness_set = nssa::public_transaction::WitnessSet::for_message(&message, &[]);
    let transaction = nssa::PublicTransaction::new(message, witness_set);
    let _response = seq_client.send_tx_public(transaction).await.unwrap();

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    let post_state_account = seq_client
        .get_account(address.to_string())
        .await
        .unwrap()
        .account;
    assert_eq!(post_state_account.program_owner, data_changer.id());
    assert_eq!(post_state_account.balance, 0);
    assert_eq!(post_state_account.data, vec![0]);
    assert_eq!(post_state_account.nonce, 0);

    info!("Success!");
}

pub async fn test_authenticated_transfer_initialize_function() {
    info!("test initialize account for authenticated transfer");
    let command = Command::AuthenticatedTransferInitializePublicAccount {};

    let SubcommandReturnValue::RegisterAccount { addr } =
        wallet::execute_subcommand(command).await.unwrap()
    else {
        panic!("Error creating account");
    };

    info!("Checking correct execution");
    let wallet_config = fetch_config().await.unwrap();
    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();
    let account = seq_client
        .get_account(addr.to_string())
        .await
        .unwrap()
        .account;

    let expected_program_owner = Program::authenticated_transfer_program().id();
    let expected_nonce = 1;
    let expected_balance = 0;

    assert_eq!(account.program_owner, expected_program_owner);
    assert_eq!(account.balance, expected_balance);
    assert_eq!(account.nonce, expected_nonce);
    assert!(account.data.is_empty());

    info!("Success!");
}

pub async fn test_pinata_private_receiver() {
    info!("test_pinata_private_receiver");
    let pinata_addr = "cafe".repeat(16);
    let pinata_prize = 150;
    let solution = 989106;

    let command = Command::PinataProgram(PinataProgramSubcommand::Private(
        PinataProgramSubcommandPrivate::ClaimPrivateOwned {
            pinata_addr: pinata_addr.clone(),
            winner_addr: ACC_SENDER_PRIVATE.to_string(),
            solution,
        },
    ));

    let wallet_config = fetch_config().await.unwrap();

    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

    let pinata_balance_pre = seq_client
        .get_account_balance(pinata_addr.clone())
        .await
        .unwrap()
        .balance;

    let SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash } =
        wallet::execute_subcommand(command).await.unwrap()
    else {
        panic!("invalid subcommand return value");
    };

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    info!("Checking correct balance move");
    let pinata_balance_post = seq_client
        .get_account_balance(pinata_addr.clone())
        .await
        .unwrap()
        .balance;

    let command = Command::Account(AccountSubcommand::Fetch(FetchSubcommand::PrivateAccount {
        tx_hash: tx_hash.clone(),
        acc_addr: ACC_SENDER_PRIVATE.to_string(),
        output_id: 0,
    }));
    wallet::execute_subcommand(command).await.unwrap();

    let wallet_config = fetch_config().await.unwrap();
    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();
    let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
        .await
        .unwrap();

    let new_commitment1 = wallet_storage
        .get_private_account_commitment(&ACC_SENDER_PRIVATE.parse().unwrap())
        .unwrap();
    assert!(verify_commitment_is_in_state(new_commitment1, &seq_client).await);

    assert_eq!(pinata_balance_post, pinata_balance_pre - pinata_prize);

    info!("Success!");
}

pub async fn test_pinata_private_receiver_new_account() {
    info!("test_pinata_private_receiver");
    let pinata_addr = "cafe".repeat(16);
    let pinata_prize = 150;
    let solution = 989106;

    // Create new account for the token supply holder (private)
    let SubcommandReturnValue::RegisterAccount { addr: winner_addr } = wallet::execute_subcommand(
        Command::Account(AccountSubcommand::Register(RegisterSubcommand::Private {})),
    )
    .await
    .unwrap() else {
        panic!("invalid subcommand return value");
    };

    let command = Command::PinataProgram(PinataProgramSubcommand::Private(
        PinataProgramSubcommandPrivate::ClaimPrivateOwned {
            pinata_addr: pinata_addr.clone(),
            winner_addr: winner_addr.to_string(),
            solution,
        },
    ));

    let wallet_config = fetch_config().await.unwrap();

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

    let wallet_config = fetch_config().await.unwrap();
    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();
    let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
        .await
        .unwrap();

    let new_commitment1 = wallet_storage
        .get_private_account_commitment(&winner_addr)
        .unwrap();
    assert!(verify_commitment_is_in_state(new_commitment1, &seq_client).await);

    assert_eq!(pinata_balance_post, pinata_balance_pre - pinata_prize);

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
        "test_pinata" => {
            test_cleanup_wrap!(home_dir, test_pinata);
        }
        "test_program_deployment" => {
            test_cleanup_wrap!(home_dir, test_program_deployment);
        }
        "test_authenticated_transfer_initialize_function" => {
            test_cleanup_wrap!(home_dir, test_authenticated_transfer_initialize_function);
        }
        "test_pinata_private_receiver" => {
            test_cleanup_wrap!(home_dir, test_pinata_private_receiver);
        }
        "test_success_token_program_private_owned" => {
            test_cleanup_wrap!(home_dir, test_success_token_program_private_owned);
        }
        "test_success_token_program_private_claiming_path" => {
            test_cleanup_wrap!(home_dir, test_success_token_program_private_claiming_path);
        }
        "test_pinata_private_receiver_new_account" => {
            test_cleanup_wrap!(home_dir, test_pinata_private_receiver_new_account);
        }
        "test_success_private_transfer_to_another_owned_account_cont_run_path" => {
            test_cleanup_wrap!(
                home_dir,
                test_success_private_transfer_to_another_owned_account_cont_run_path
            );
        }
        "test_success_token_program_shielded_owned" => {
            test_cleanup_wrap!(home_dir, test_success_token_program_shielded_owned);
        }
        "test_success_token_program_deshielded_owned" => {
            test_cleanup_wrap!(home_dir, test_success_token_program_deshielded_owned);
        }
        "all" => {
            test_cleanup_wrap!(home_dir, test_success_move_to_another_account);
            test_cleanup_wrap!(home_dir, test_success);
            test_cleanup_wrap!(home_dir, test_failure);
            test_cleanup_wrap!(home_dir, test_get_account);
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
            test_cleanup_wrap!(home_dir, test_success_token_program_shielded_owned);
            test_cleanup_wrap!(home_dir, test_pinata);
            test_cleanup_wrap!(home_dir, test_program_deployment);
            test_cleanup_wrap!(home_dir, test_authenticated_transfer_initialize_function);
            test_cleanup_wrap!(home_dir, test_pinata_private_receiver);
            test_cleanup_wrap!(home_dir, test_success_token_program_private_owned);
            test_cleanup_wrap!(home_dir, test_success_token_program_deshielded_owned);
            test_cleanup_wrap!(home_dir, test_success_token_program_private_claiming_path);
            test_cleanup_wrap!(home_dir, test_pinata_private_receiver_new_account);
            test_cleanup_wrap!(
                home_dir,
                test_success_private_transfer_to_another_owned_account_cont_run_path
            );
        }
        "all_private" => {
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
            test_cleanup_wrap!(home_dir, test_pinata_private_receiver);
            test_cleanup_wrap!(home_dir, test_success_token_program_private_owned);
            test_cleanup_wrap!(home_dir, test_success_token_program_private_claiming_path);
            test_cleanup_wrap!(home_dir, test_pinata_private_receiver_new_account);
            test_cleanup_wrap!(
                home_dir,
                test_success_private_transfer_to_another_owned_account_cont_run_path
            );
        }
        _ => {
            anyhow::bail!("Unknown test name");
        }
    }

    Ok(())
}

async fn fetch_privacy_preserving_tx(
    seq_client: &SequencerClient,
    tx_hash: String,
) -> PrivacyPreservingTransaction {
    let transaction_encoded = seq_client
        .get_transaction_by_hash(tx_hash.clone())
        .await
        .unwrap()
        .transaction
        .unwrap();

    let tx_base64_decode = BASE64.decode(transaction_encoded).unwrap();
    match NSSATransaction::try_from(
        &borsh::from_slice::<EncodedTransaction>(&tx_base64_decode).unwrap(),
    )
    .unwrap()
    {
        NSSATransaction::PrivacyPreserving(privacy_preserving_transaction) => {
            privacy_preserving_transaction
        }
        _ => panic!("Invalid tx type"),
    }
}

async fn verify_commitment_is_in_state(
    commitment: Commitment,
    seq_client: &SequencerClient,
) -> bool {
    matches!(
        seq_client.get_proof_for_commitment(commitment).await,
        Ok(Some(_))
    )
}

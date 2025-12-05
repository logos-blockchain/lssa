use std::{
    collections::HashMap,
    path::PathBuf,
    pin::Pin,
    str::FromStr,
    time::{Duration, Instant},
};

use actix_web::dev::ServerHandle;
use anyhow::Result;
use common::{PINATA_BASE58, sequencer_client::SequencerClient};
use key_protocol::key_management::key_tree::chain_index::ChainIndex;
use log::info;
use nssa::{AccountId, ProgramDeploymentTransaction, program::Program};
use nssa_core::{NullifierPublicKey, encryption::shared_key_derivation::Secp256k1Point};
use sequencer_runner::startup_sequencer;
use tempfile::TempDir;
use tokio::task::JoinHandle;
use wallet::{
    WalletCore,
    cli::{
        Command, SubcommandReturnValue,
        account::{AccountSubcommand, NewSubcommand},
        config::ConfigSubcommand,
        programs::{
            native_token_transfer::AuthTransferSubcommand, pinata::PinataProgramAgnosticSubcommand,
            token::TokenProgramAgnosticSubcommand,
        },
    },
    config::PersistentStorage,
    helperfunctions::{fetch_config, fetch_persistent_storage},
};

use crate::{
    ACC_RECEIVER, ACC_RECEIVER_PRIVATE, ACC_SENDER, ACC_SENDER_PRIVATE,
    NSSA_PROGRAM_FOR_TEST_DATA_CHANGER, TIME_TO_WAIT_FOR_BLOCK_SECONDS,
    fetch_privacy_preserving_tx, make_private_account_input_from_str,
    make_public_account_input_from_str, post_test, pre_test,
    replace_home_dir_with_temp_dir_in_configs, tps_test_utils::TpsTestManager,
    verify_commitment_is_in_state,
};

type TestFunction = fn(PathBuf) -> Pin<Box<dyn Future<Output = ()>>>;

pub fn prepare_function_map() -> HashMap<String, TestFunction> {
    let mut function_map: HashMap<String, TestFunction> = HashMap::new();

    #[nssa_integration_test]
    pub async fn test_success() {
        info!("########## test_success ##########");
        let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
            from: make_public_account_input_from_str(ACC_SENDER),
            to: Some(make_public_account_input_from_str(ACC_RECEIVER)),
            to_npk: None,
            to_ipk: None,
            amount: 100,
        });

        let wallet_config = fetch_config().await.unwrap();

        let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

        wallet::cli::execute_subcommand(command).await.unwrap();

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

    #[nssa_integration_test]
    pub async fn test_success_move_to_another_account() {
        info!("########## test_success_move_to_another_account ##########");
        let command = Command::Account(AccountSubcommand::New(NewSubcommand::Public {
            cci: ChainIndex::root(),
        }));

        let wallet_config = fetch_config().await.unwrap();

        let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

        wallet::cli::execute_subcommand(command).await.unwrap();

        let PersistentStorage {
            accounts: persistent_accounts,
            last_synced_block: _,
        } = fetch_persistent_storage().await.unwrap();

        let mut new_persistent_account_id = String::new();

        for per_acc in persistent_accounts {
            if (per_acc.account_id().to_string() != ACC_RECEIVER)
                && (per_acc.account_id().to_string() != ACC_SENDER)
            {
                new_persistent_account_id = per_acc.account_id().to_string();
            }
        }

        if new_persistent_account_id == String::new() {
            panic!("Failed to produce new account, not present in persistent accounts");
        }

        let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
            from: make_public_account_input_from_str(ACC_SENDER),
            to: Some(make_public_account_input_from_str(
                &new_persistent_account_id,
            )),
            to_npk: None,
            to_ipk: None,
            amount: 100,
        });

        wallet::cli::execute_subcommand(command).await.unwrap();

        info!("Waiting for next block creation");
        tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

        info!("Checking correct balance move");
        let acc_1_balance = seq_client
            .get_account_balance(ACC_SENDER.to_string())
            .await
            .unwrap();
        let acc_2_balance = seq_client
            .get_account_balance(new_persistent_account_id)
            .await
            .unwrap();

        info!("Balance of sender : {acc_1_balance:#?}");
        info!("Balance of receiver : {acc_2_balance:#?}");

        assert_eq!(acc_1_balance.balance, 9900);
        assert_eq!(acc_2_balance.balance, 100);

        info!("Success!");
    }

    #[nssa_integration_test]
    pub async fn test_failure() {
        info!("########## test_failure ##########");
        let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
            from: make_public_account_input_from_str(ACC_SENDER),
            to: Some(make_public_account_input_from_str(ACC_RECEIVER)),
            to_npk: None,
            to_ipk: None,
            amount: 1000000,
        });

        let wallet_config = fetch_config().await.unwrap();

        let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

        let failed_send = wallet::cli::execute_subcommand(command).await;

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

    #[nssa_integration_test]
    pub async fn test_success_two_transactions() {
        info!("########## test_success_two_transactions ##########");
        let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
            from: make_public_account_input_from_str(ACC_SENDER),
            to: Some(make_public_account_input_from_str(ACC_RECEIVER)),
            to_npk: None,
            to_ipk: None,
            amount: 100,
        });

        let wallet_config = fetch_config().await.unwrap();

        let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

        wallet::cli::execute_subcommand(command).await.unwrap();

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

        let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
            from: make_public_account_input_from_str(ACC_SENDER),
            to: Some(make_public_account_input_from_str(ACC_RECEIVER)),
            to_npk: None,
            to_ipk: None,
            amount: 100,
        });

        wallet::cli::execute_subcommand(command).await.unwrap();

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

    #[nssa_integration_test]
    pub async fn test_get_account() {
        info!("########## test_get_account ##########");
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

    /// This test creates a new token using the token program. After creating the token, the test
    /// executes a token transfer to a new account.
    #[nssa_integration_test]
    pub async fn test_success_token_program() {
        info!("########## test_success_token_program ##########");
        let wallet_config = fetch_config().await.unwrap();

        // Create new account for the token definition
        let SubcommandReturnValue::RegisterAccount {
            account_id: definition_account_id,
        } = wallet::cli::execute_subcommand(Command::Account(AccountSubcommand::New(
            NewSubcommand::Public {
                cci: ChainIndex::root(),
            },
        )))
        .await
        .unwrap()
        else {
            panic!("invalid subcommand return value");
        };
        // Create new account for the token supply holder
        let SubcommandReturnValue::RegisterAccount {
            account_id: supply_account_id,
        } = wallet::cli::execute_subcommand(Command::Account(AccountSubcommand::New(
            NewSubcommand::Public {
                cci: ChainIndex::root(),
            },
        )))
        .await
        .unwrap()
        else {
            panic!("invalid subcommand return value");
        };
        // Create new account for receiving a token transaction
        let SubcommandReturnValue::RegisterAccount {
            account_id: recipient_account_id,
        } = wallet::cli::execute_subcommand(Command::Account(AccountSubcommand::New(
            NewSubcommand::Public {
                cci: ChainIndex::root(),
            },
        )))
        .await
        .unwrap()
        else {
            panic!("invalid subcommand return value");
        };

        // Create new token
        let subcommand = TokenProgramAgnosticSubcommand::New {
            definition_account_id: make_public_account_input_from_str(
                &definition_account_id.to_string(),
            ),
            supply_account_id: make_public_account_input_from_str(&supply_account_id.to_string()),
            name: "A NAME".to_string(),
            total_supply: 37,
        };
        wallet::cli::execute_subcommand(Command::Token(subcommand))
            .await
            .unwrap();
        info!("Waiting for next block creation");
        tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

        let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

        // Check the status of the token definition account is the expected after the execution
        let definition_acc = seq_client
            .get_account(definition_account_id.to_string())
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

        // Check the status of the token holding account with the total supply is the expected after
        // the execution
        let supply_acc = seq_client
            .get_account(supply_account_id.to_string())
            .await
            .unwrap()
            .account;

        // The account must be owned by the token program
        assert_eq!(supply_acc.program_owner, Program::token().id());
        // The data of a token definition account has the following layout:
        // [ 0x01 || corresponding_token_definition_id (32 bytes) || balance (little endian 16
        // bytes) ] First byte of the data equal to 1 means it's a token holding account
        assert_eq!(supply_acc.data[0], 1);
        // Bytes from 1 to 33 represent the id of the token this account is associated with.
        // In this example, this is a token account of the newly created token, so it is expected
        // to be equal to the account_id of the token definition account.
        assert_eq!(&supply_acc.data[1..33], definition_account_id.to_bytes());
        assert_eq!(
            u128::from_le_bytes(supply_acc.data[33..].try_into().unwrap()),
            37
        );

        // Transfer 7 tokens from `supply_acc` to the account at account_id `recipient_account_id`
        let subcommand = TokenProgramAgnosticSubcommand::Send {
            from: make_public_account_input_from_str(&supply_account_id.to_string()),
            to: Some(make_public_account_input_from_str(
                &recipient_account_id.to_string(),
            )),
            to_npk: None,
            to_ipk: None,
            amount: 7,
        };

        wallet::cli::execute_subcommand(Command::Token(subcommand))
            .await
            .unwrap();
        info!("Waiting for next block creation");
        tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

        // Check the status of the account at `supply_account_id` is the expected after the
        // execution
        let supply_acc = seq_client
            .get_account(supply_account_id.to_string())
            .await
            .unwrap()
            .account;
        // The account must be owned by the token program
        assert_eq!(supply_acc.program_owner, Program::token().id());
        // First byte equal to 1 means it's a token holding account
        assert_eq!(supply_acc.data[0], 1);
        // Bytes from 1 to 33 represent the id of the token this account is associated with.
        assert_eq!(&supply_acc.data[1..33], definition_account_id.to_bytes());
        assert_eq!(
            u128::from_le_bytes(supply_acc.data[33..].try_into().unwrap()),
            30
        );

        // Check the status of the account at `recipient_account_id` is the expected after the
        // execution
        let recipient_acc = seq_client
            .get_account(recipient_account_id.to_string())
            .await
            .unwrap()
            .account;

        // The account must be owned by the token program
        assert_eq!(recipient_acc.program_owner, Program::token().id());
        // First byte equal to 1 means it's a token holding account
        assert_eq!(recipient_acc.data[0], 1);
        // Bytes from 1 to 33 represent the id of the token this account is associated with.
        assert_eq!(&recipient_acc.data[1..33], definition_account_id.to_bytes());
        assert_eq!(
            u128::from_le_bytes(recipient_acc.data[33..].try_into().unwrap()),
            7
        );
    }

    /// This test creates a new private token using the token program. After creating the token, the
    /// test executes a private token transfer to a new account. All accounts are owned except
    /// definition.
    #[nssa_integration_test]
    pub async fn test_success_token_program_private_owned() {
        info!("########## test_success_token_program_private_owned ##########");
        let wallet_config = fetch_config().await.unwrap();

        // Create new account for the token definition (public)
        let SubcommandReturnValue::RegisterAccount {
            account_id: definition_account_id,
        } = wallet::cli::execute_subcommand(Command::Account(AccountSubcommand::New(
            NewSubcommand::Public {
                cci: ChainIndex::root(),
            },
        )))
        .await
        .unwrap()
        else {
            panic!("invalid subcommand return value");
        };
        // Create new account for the token supply holder (private)
        let SubcommandReturnValue::RegisterAccount {
            account_id: supply_account_id,
        } = wallet::cli::execute_subcommand(Command::Account(AccountSubcommand::New(
            NewSubcommand::Private {
                cci: ChainIndex::root(),
            },
        )))
        .await
        .unwrap()
        else {
            panic!("invalid subcommand return value");
        };
        // Create new account for receiving a token transaction
        let SubcommandReturnValue::RegisterAccount {
            account_id: recipient_account_id,
        } = wallet::cli::execute_subcommand(Command::Account(AccountSubcommand::New(
            NewSubcommand::Private {
                cci: ChainIndex::root(),
            },
        )))
        .await
        .unwrap()
        else {
            panic!("invalid subcommand return value");
        };

        // Create new token
        let subcommand = TokenProgramAgnosticSubcommand::New {
            definition_account_id: make_public_account_input_from_str(
                &definition_account_id.to_string(),
            ),
            supply_account_id: make_private_account_input_from_str(&supply_account_id.to_string()),
            name: "A NAME".to_string(),
            total_supply: 37,
        };

        wallet::cli::execute_subcommand(Command::Token(subcommand))
            .await
            .unwrap();

        info!("Waiting for next block creation");
        tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

        let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

        // Check the status of the token definition account is the expected after the execution
        let definition_acc = seq_client
            .get_account(definition_account_id.to_string())
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
            .get_private_account_commitment(&supply_account_id)
            .unwrap();
        assert!(verify_commitment_is_in_state(new_commitment1, &seq_client).await);

        // Transfer 7 tokens from `supply_acc` to the account at account_id `recipient_account_id`
        let subcommand = TokenProgramAgnosticSubcommand::Send {
            from: make_private_account_input_from_str(&supply_account_id.to_string()),
            to: Some(make_private_account_input_from_str(
                &recipient_account_id.to_string(),
            )),
            to_npk: None,
            to_ipk: None,
            amount: 7,
        };

        wallet::cli::execute_subcommand(Command::Token(subcommand))
            .await
            .unwrap();

        info!("Waiting for next block creation");
        tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

        let wallet_config = fetch_config().await.unwrap();
        let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
            .await
            .unwrap();

        let new_commitment1 = wallet_storage
            .get_private_account_commitment(&supply_account_id)
            .unwrap();
        assert!(verify_commitment_is_in_state(new_commitment1, &seq_client).await);

        let new_commitment2 = wallet_storage
            .get_private_account_commitment(&recipient_account_id)
            .unwrap();
        assert!(verify_commitment_is_in_state(new_commitment2, &seq_client).await);

        // Transfer additional 7 tokens from `supply_acc` to the account at account_id
        // `recipient_account_id`
        let subcommand = TokenProgramAgnosticSubcommand::Send {
            from: make_private_account_input_from_str(&supply_account_id.to_string()),
            to: Some(make_private_account_input_from_str(
                &recipient_account_id.to_string(),
            )),
            to_npk: None,
            to_ipk: None,
            amount: 7,
        };

        wallet::cli::execute_subcommand(Command::Token(subcommand))
            .await
            .unwrap();

        info!("Waiting for next block creation");
        tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

        let wallet_config = fetch_config().await.unwrap();
        let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
            .await
            .unwrap();

        let new_commitment1 = wallet_storage
            .get_private_account_commitment(&supply_account_id)
            .unwrap();
        assert!(verify_commitment_is_in_state(new_commitment1, &seq_client).await);

        let new_commitment2 = wallet_storage
            .get_private_account_commitment(&recipient_account_id)
            .unwrap();
        assert!(verify_commitment_is_in_state(new_commitment2, &seq_client).await);
    }

    /// This test creates a new private token using the token program. After creating the token, the
    /// test executes a private token transfer to a new account.
    #[nssa_integration_test]
    pub async fn test_success_token_program_private_claiming_path() {
        info!("########## test_success_token_program_private_claiming_path ##########");
        let wallet_config = fetch_config().await.unwrap();

        // Create new account for the token definition (public)
        let SubcommandReturnValue::RegisterAccount {
            account_id: definition_account_id,
        } = wallet::cli::execute_subcommand(Command::Account(AccountSubcommand::New(
            NewSubcommand::Public {
                cci: ChainIndex::root(),
            },
        )))
        .await
        .unwrap()
        else {
            panic!("invalid subcommand return value");
        };
        // Create new account for the token supply holder (private)
        let SubcommandReturnValue::RegisterAccount {
            account_id: supply_account_id,
        } = wallet::cli::execute_subcommand(Command::Account(AccountSubcommand::New(
            NewSubcommand::Private {
                cci: ChainIndex::root(),
            },
        )))
        .await
        .unwrap()
        else {
            panic!("invalid subcommand return value");
        };
        // Create new account for receiving a token transaction
        let SubcommandReturnValue::RegisterAccount {
            account_id: recipient_account_id,
        } = wallet::cli::execute_subcommand(Command::Account(AccountSubcommand::New(
            NewSubcommand::Private {
                cci: ChainIndex::root(),
            },
        )))
        .await
        .unwrap()
        else {
            panic!("invalid subcommand return value");
        };

        // Create new token
        let subcommand = TokenProgramAgnosticSubcommand::New {
            definition_account_id: make_public_account_input_from_str(
                &definition_account_id.to_string(),
            ),
            supply_account_id: make_private_account_input_from_str(&supply_account_id.to_string()),
            name: "A NAME".to_string(),
            total_supply: 37,
        };

        wallet::cli::execute_subcommand(Command::Token(subcommand))
            .await
            .unwrap();

        info!("Waiting for next block creation");
        tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

        let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

        // Check the status of the token definition account is the expected after the execution
        let definition_acc = seq_client
            .get_account(definition_account_id.to_string())
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
            .get_private_account_commitment(&supply_account_id)
            .unwrap();
        assert!(verify_commitment_is_in_state(new_commitment1, &seq_client).await);

        let (recipient_keys, _) = wallet_storage
            .storage
            .user_data
            .get_private_account(&recipient_account_id)
            .unwrap();

        // Transfer 7 tokens from `supply_acc` to the account at account_id `recipient_account_id`
        let subcommand = TokenProgramAgnosticSubcommand::Send {
            from: make_private_account_input_from_str(&supply_account_id.to_string()),
            to: None,
            to_npk: Some(hex::encode(recipient_keys.nullifer_public_key.0)),
            to_ipk: Some(hex::encode(
                recipient_keys.incoming_viewing_public_key.0.clone(),
            )),
            amount: 7,
        };

        let SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash: _ } =
            wallet::cli::execute_subcommand(Command::Token(subcommand))
                .await
                .unwrap()
        else {
            panic!("invalid subcommand return value");
        };

        info!("Waiting for next block creation");
        tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

        let command = Command::Account(AccountSubcommand::SyncPrivate {});

        wallet::cli::execute_subcommand(command).await.unwrap();

        let wallet_config = fetch_config().await.unwrap();
        let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
            .await
            .unwrap();

        let new_commitment1 = wallet_storage
            .get_private_account_commitment(&supply_account_id)
            .unwrap();
        assert!(verify_commitment_is_in_state(new_commitment1, &seq_client).await);

        let new_commitment2 = wallet_storage
            .get_private_account_commitment(&recipient_account_id)
            .unwrap();
        assert!(verify_commitment_is_in_state(new_commitment2, &seq_client).await);
    }

    /// This test creates a new public token using the token program. After creating the token, the
    /// test executes a shielded token transfer to a new account. All accounts are owned except
    /// definition.
    #[nssa_integration_test]
    pub async fn test_success_token_program_shielded_owned() {
        info!("########## test_success_token_program_shielded_owned ##########");
        let wallet_config = fetch_config().await.unwrap();

        // Create new account for the token definition (public)
        let SubcommandReturnValue::RegisterAccount {
            account_id: definition_account_id,
        } = wallet::cli::execute_subcommand(Command::Account(AccountSubcommand::New(
            NewSubcommand::Public {
                cci: ChainIndex::root(),
            },
        )))
        .await
        .unwrap()
        else {
            panic!("invalid subcommand return value");
        };
        // Create new account for the token supply holder (public)
        let SubcommandReturnValue::RegisterAccount {
            account_id: supply_account_id,
        } = wallet::cli::execute_subcommand(Command::Account(AccountSubcommand::New(
            NewSubcommand::Public {
                cci: ChainIndex::root(),
            },
        )))
        .await
        .unwrap()
        else {
            panic!("invalid subcommand return value");
        };
        // Create new account for receiving a token transaction
        let SubcommandReturnValue::RegisterAccount {
            account_id: recipient_account_id,
        } = wallet::cli::execute_subcommand(Command::Account(AccountSubcommand::New(
            NewSubcommand::Private {
                cci: ChainIndex::root(),
            },
        )))
        .await
        .unwrap()
        else {
            panic!("invalid subcommand return value");
        };

        // Create new token
        let subcommand = TokenProgramAgnosticSubcommand::New {
            definition_account_id: make_public_account_input_from_str(
                &definition_account_id.to_string(),
            ),
            supply_account_id: make_public_account_input_from_str(&supply_account_id.to_string()),
            name: "A NAME".to_string(),
            total_supply: 37,
        };

        wallet::cli::execute_subcommand(Command::Token(subcommand))
            .await
            .unwrap();

        info!("Waiting for next block creation");
        tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

        let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

        // Check the status of the token definition account is the expected after the execution
        let definition_acc = seq_client
            .get_account(definition_account_id.to_string())
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

        // Transfer 7 tokens from `supply_acc` to the account at account_id `recipient_account_id`
        let subcommand = TokenProgramAgnosticSubcommand::Send {
            from: make_public_account_input_from_str(&supply_account_id.to_string()),
            to: Some(make_private_account_input_from_str(
                &recipient_account_id.to_string(),
            )),
            to_npk: None,
            to_ipk: None,
            amount: 7,
        };

        wallet::cli::execute_subcommand(Command::Token(subcommand))
            .await
            .unwrap();

        info!("Waiting for next block creation");
        tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

        let wallet_config = fetch_config().await.unwrap();
        let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
            .await
            .unwrap();

        let new_commitment2 = wallet_storage
            .get_private_account_commitment(&recipient_account_id)
            .unwrap();
        assert!(verify_commitment_is_in_state(new_commitment2, &seq_client).await);

        // Transfer additional 7 tokens from `supply_acc` to the account at account_id
        // `recipient_account_id`
        let subcommand = TokenProgramAgnosticSubcommand::Send {
            from: make_public_account_input_from_str(&supply_account_id.to_string()),
            to: Some(make_private_account_input_from_str(
                &recipient_account_id.to_string(),
            )),
            to_npk: None,
            to_ipk: None,
            amount: 7,
        };

        wallet::cli::execute_subcommand(Command::Token(subcommand))
            .await
            .unwrap();

        info!("Waiting for next block creation");
        tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

        let wallet_config = fetch_config().await.unwrap();
        let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
            .await
            .unwrap();

        let new_commitment2 = wallet_storage
            .get_private_account_commitment(&recipient_account_id)
            .unwrap();
        assert!(verify_commitment_is_in_state(new_commitment2, &seq_client).await);
    }

    /// This test creates a new private token using the token program. After creating the token, the
    /// test executes a deshielded token transfer to a new account. All accounts are owned
    /// except definition.
    #[nssa_integration_test]
    pub async fn test_success_token_program_deshielded_owned() {
        info!("########## test_success_token_program_deshielded_owned ##########");
        let wallet_config = fetch_config().await.unwrap();

        // Create new account for the token definition (public)
        let SubcommandReturnValue::RegisterAccount {
            account_id: definition_account_id,
        } = wallet::cli::execute_subcommand(Command::Account(AccountSubcommand::New(
            NewSubcommand::Public {
                cci: ChainIndex::root(),
            },
        )))
        .await
        .unwrap()
        else {
            panic!("invalid subcommand return value");
        };
        // Create new account for the token supply holder (private)
        let SubcommandReturnValue::RegisterAccount {
            account_id: supply_account_id,
        } = wallet::cli::execute_subcommand(Command::Account(AccountSubcommand::New(
            NewSubcommand::Private {
                cci: ChainIndex::root(),
            },
        )))
        .await
        .unwrap()
        else {
            panic!("invalid subcommand return value");
        };
        // Create new account for receiving a token transaction
        let SubcommandReturnValue::RegisterAccount {
            account_id: recipient_account_id,
        } = wallet::cli::execute_subcommand(Command::Account(AccountSubcommand::New(
            NewSubcommand::Public {
                cci: ChainIndex::root(),
            },
        )))
        .await
        .unwrap()
        else {
            panic!("invalid subcommand return value");
        };

        // Create new token
        let subcommand = TokenProgramAgnosticSubcommand::New {
            definition_account_id: make_public_account_input_from_str(
                &definition_account_id.to_string(),
            ),
            supply_account_id: make_private_account_input_from_str(&supply_account_id.to_string()),
            name: "A NAME".to_string(),
            total_supply: 37,
        };

        wallet::cli::execute_subcommand(Command::Token(subcommand))
            .await
            .unwrap();

        info!("Waiting for next block creation");
        tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

        let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

        // Check the status of the token definition account is the expected after the execution
        let definition_acc = seq_client
            .get_account(definition_account_id.to_string())
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
            .get_private_account_commitment(&supply_account_id)
            .unwrap();
        assert!(verify_commitment_is_in_state(new_commitment1, &seq_client).await);

        // Transfer 7 tokens from `supply_acc` to the account at account_id `recipient_account_id`
        let subcommand = TokenProgramAgnosticSubcommand::Send {
            from: make_private_account_input_from_str(&supply_account_id.to_string()),
            to: Some(make_public_account_input_from_str(
                &recipient_account_id.to_string(),
            )),
            to_npk: None,
            to_ipk: None,
            amount: 7,
        };

        wallet::cli::execute_subcommand(Command::Token(subcommand))
            .await
            .unwrap();

        info!("Waiting for next block creation");
        tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

        let wallet_config = fetch_config().await.unwrap();
        let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
            .await
            .unwrap();

        let new_commitment1 = wallet_storage
            .get_private_account_commitment(&supply_account_id)
            .unwrap();
        assert!(verify_commitment_is_in_state(new_commitment1, &seq_client).await);

        // Transfer additional 7 tokens from `supply_acc` to the account at account_id
        // `recipient_account_id`
        let subcommand = TokenProgramAgnosticSubcommand::Send {
            from: make_private_account_input_from_str(&supply_account_id.to_string()),
            to: Some(make_public_account_input_from_str(
                &recipient_account_id.to_string(),
            )),
            to_npk: None,
            to_ipk: None,
            amount: 7,
        };

        wallet::cli::execute_subcommand(Command::Token(subcommand))
            .await
            .unwrap();

        info!("Waiting for next block creation");
        tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

        let wallet_config = fetch_config().await.unwrap();
        let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
            .await
            .unwrap();

        let new_commitment1 = wallet_storage
            .get_private_account_commitment(&supply_account_id)
            .unwrap();
        assert!(verify_commitment_is_in_state(new_commitment1, &seq_client).await);
    }

    #[nssa_integration_test]
    pub async fn test_success_private_transfer_to_another_owned_account() {
        info!("########## test_success_private_transfer_to_another_owned_account ##########");
        let from: AccountId = ACC_SENDER_PRIVATE.parse().unwrap();
        let to: AccountId = ACC_RECEIVER_PRIVATE.parse().unwrap();

        let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
            from: make_private_account_input_from_str(&from.to_string()),
            to: Some(make_private_account_input_from_str(&to.to_string())),
            to_npk: None,
            to_ipk: None,
            amount: 100,
        });

        wallet::cli::execute_subcommand(command).await.unwrap();

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

    #[nssa_integration_test]
    pub async fn test_success_private_transfer_to_another_foreign_account() {
        info!("########## test_success_private_transfer_to_another_foreign_account ##########");
        let from: AccountId = ACC_SENDER_PRIVATE.parse().unwrap();
        let to_npk = NullifierPublicKey([42; 32]);
        let to_npk_string = hex::encode(to_npk.0);
        let to_ipk = Secp256k1Point::from_scalar(to_npk.0);

        let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
            from: make_private_account_input_from_str(&from.to_string()),
            to: None,
            to_npk: Some(to_npk_string),
            to_ipk: Some(hex::encode(to_ipk.0)),
            amount: 100,
        });

        let SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash } =
            wallet::cli::execute_subcommand(command).await.unwrap()
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

    #[nssa_integration_test]
    pub async fn test_success_private_transfer_to_another_owned_account_claiming_path() {
        info!(
            "########## test_success_private_transfer_to_another_owned_account_claiming_path ##########"
        );
        let from: AccountId = ACC_SENDER_PRIVATE.parse().unwrap();

        let command = Command::Account(AccountSubcommand::New(NewSubcommand::Private {
            cci: ChainIndex::root(),
        }));

        let sub_ret = wallet::cli::execute_subcommand(command).await.unwrap();
        let SubcommandReturnValue::RegisterAccount {
            account_id: to_account_id,
        } = sub_ret
        else {
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
            .get_private_account(&to_account_id)
            .cloned()
            .unwrap();

        let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
            from: make_private_account_input_from_str(&from.to_string()),
            to: None,
            to_npk: Some(hex::encode(to_keys.nullifer_public_key.0)),
            to_ipk: Some(hex::encode(to_keys.incoming_viewing_public_key.0)),
            amount: 100,
        });

        let sub_ret = wallet::cli::execute_subcommand(command).await.unwrap();
        let SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash } = sub_ret else {
            panic!("FAILED TO SEND TX");
        };

        let tx = fetch_privacy_preserving_tx(&seq_client, tx_hash.clone()).await;

        let command = Command::Account(AccountSubcommand::SyncPrivate {});
        wallet::cli::execute_subcommand(command).await.unwrap();
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

        let to_res_acc = wallet_storage.get_account_private(&to_account_id).unwrap();

        assert_eq!(to_res_acc.balance, 100);

        info!("Success!");
    }

    // #[nssa_integration_test]
    // pub async fn test_success_private_transfer_to_another_owned_account_cont_run_path() {
    //     info!(
    //         "########## test_success_private_transfer_to_another_owned_account_cont_run_path
    // ##########"     );
    //     let continious_run_handle = tokio::spawn(wallet::cli::execute_continious_run());

    //     let from: AccountId = ACC_SENDER_PRIVATE.parse().unwrap();

    //     let command = Command::Account(AccountSubcommand::New(NewSubcommand::Private {}));

    //     let sub_ret = wallet::cli::execute_subcommand(command).await.unwrap();
    //     let SubcommandReturnValue::RegisterAccount {
    //         account_id: to_account_id,
    //     } = sub_ret
    //     else {
    //         panic!("FAILED TO REGISTER ACCOUNT");
    //     };

    //     let wallet_config = fetch_config().await.unwrap();
    //     let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();
    //     let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config.clone())
    //         .await
    //         .unwrap();

    //     let (to_keys, _) = wallet_storage
    //         .storage
    //         .user_data
    //         .user_private_accounts
    //         .get(&to_account_id)
    //         .cloned()
    //         .unwrap();

    //     let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
    //         from: make_private_account_input_from_str(&from.to_string()),
    //         to: None,
    //         to_npk: Some(hex::encode(to_keys.nullifer_public_key.0)),
    //         to_ipk: Some(hex::encode(to_keys.incoming_viewing_public_key.0)),
    //         amount: 100,
    //     });

    //     let sub_ret = wallet::cli::execute_subcommand(command).await.unwrap();
    //     let SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash } = sub_ret else {
    //         panic!("FAILED TO SEND TX");
    //     };

    //     let tx = fetch_privacy_preserving_tx(&seq_client, tx_hash.clone()).await;
    //     println!("Waiting for next blocks to check if continoius run fetch account");
    //     tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;
    //     tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    //     let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
    //         .await
    //         .unwrap();

    //     assert_eq!(tx.message.new_commitments.len(), 2);
    //     for commitment in tx.message.new_commitments.into_iter() {
    //         assert!(verify_commitment_is_in_state(commitment, &seq_client).await);
    //     }

    //     let to_res_acc = wallet_storage.get_account_private(&to_account_id).unwrap();

    //     assert_eq!(to_res_acc.balance, 100);

    //     continious_run_handle.abort();

    //     info!("Success!");
    // }

    #[nssa_integration_test]
    pub async fn test_success_deshielded_transfer_to_another_account() {
        info!("########## test_success_deshielded_transfer_to_another_account ##########");
        let from: AccountId = ACC_SENDER_PRIVATE.parse().unwrap();
        let to: AccountId = ACC_RECEIVER.parse().unwrap();

        let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
            from: make_private_account_input_from_str(&from.to_string()),
            to: Some(make_public_account_input_from_str(&to.to_string())),
            to_npk: None,
            to_ipk: None,
            amount: 100,
        });

        let wallet_config = fetch_config().await.unwrap();
        let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();
        let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config.clone())
            .await
            .unwrap();

        let from_acc = wallet_storage.get_account_private(&from).unwrap();
        assert_eq!(from_acc.balance, 10000);

        wallet::cli::execute_subcommand(command).await.unwrap();

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

    #[nssa_integration_test]
    pub async fn test_success_shielded_transfer_to_another_owned_account() {
        info!("########## test_success_shielded_transfer_to_another_owned_account ##########");
        let from: AccountId = ACC_SENDER.parse().unwrap();
        let to: AccountId = ACC_RECEIVER_PRIVATE.parse().unwrap();

        let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
            from: make_public_account_input_from_str(&from.to_string()),
            to: Some(make_private_account_input_from_str(&to.to_string())),
            to_npk: None,
            to_ipk: None,
            amount: 100,
        });

        let wallet_config = fetch_config().await.unwrap();
        let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

        wallet::cli::execute_subcommand(command).await.unwrap();

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

    #[nssa_integration_test]
    pub async fn test_success_shielded_transfer_to_another_foreign_account() {
        info!("########## test_success_shielded_transfer_to_another_foreign_account ##########");
        let to_npk = NullifierPublicKey([42; 32]);
        let to_npk_string = hex::encode(to_npk.0);
        let to_ipk = Secp256k1Point::from_scalar(to_npk.0);
        let from: AccountId = ACC_SENDER.parse().unwrap();

        let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
            from: make_public_account_input_from_str(&from.to_string()),
            to: None,
            to_npk: Some(to_npk_string),
            to_ipk: Some(hex::encode(to_ipk.0)),
            amount: 100,
        });

        let wallet_config = fetch_config().await.unwrap();

        let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

        let SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash } =
            wallet::cli::execute_subcommand(command).await.unwrap()
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

    #[nssa_integration_test]
    pub async fn test_pinata() {
        info!("########## test_pinata ##########");
        let pinata_account_id = PINATA_BASE58;

        let pinata_prize = 150;
        let command = Command::Pinata(PinataProgramAgnosticSubcommand::Claim {
            to: make_public_account_input_from_str(ACC_SENDER),
        });

        let wallet_config = fetch_config().await.unwrap();

        let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

        let pinata_balance_pre = seq_client
            .get_account_balance(pinata_account_id.to_string())
            .await
            .unwrap()
            .balance;

        wallet::cli::execute_subcommand(command).await.unwrap();

        info!("Waiting for next block creation");
        tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

        info!("Checking correct balance move");
        let pinata_balance_post = seq_client
            .get_account_balance(pinata_account_id.to_string())
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

    #[nssa_integration_test]
    pub async fn test_program_deployment() {
        info!("########## test program deployment ##########");
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
        // changer program (NSSA account claiming mechanism) with data equal to [0] (due to program
        // logic)
        let data_changer = Program::new(bytecode).unwrap();
        let account_id: AccountId = "11".repeat(16).parse().unwrap();
        let message = nssa::public_transaction::Message::try_new(
            data_changer.id(),
            vec![account_id],
            vec![],
            (),
        )
        .unwrap();
        let witness_set = nssa::public_transaction::WitnessSet::for_message(&message, &[]);
        let transaction = nssa::PublicTransaction::new(message, witness_set);
        let _response = seq_client.send_tx_public(transaction).await.unwrap();

        info!("Waiting for next block creation");
        tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

        let post_state_account = seq_client
            .get_account(account_id.to_string())
            .await
            .unwrap()
            .account;
        assert_eq!(post_state_account.program_owner, data_changer.id());
        assert_eq!(post_state_account.balance, 0);
        assert_eq!(post_state_account.data, vec![0]);
        assert_eq!(post_state_account.nonce, 0);

        info!("Success!");
    }

    #[nssa_integration_test]
    pub async fn test_authenticated_transfer_initialize_function() {
        info!("########## test initialize account for authenticated transfer ##########");
        let command = Command::Account(AccountSubcommand::New(NewSubcommand::Public {
            cci: ChainIndex::root(),
        }));
        let SubcommandReturnValue::RegisterAccount { account_id } =
            wallet::cli::execute_subcommand(command).await.unwrap()
        else {
            panic!("Error creating account");
        };

        let command = Command::AuthTransfer(AuthTransferSubcommand::Init {
            account_id: make_public_account_input_from_str(&account_id.to_string()),
        });
        wallet::cli::execute_subcommand(command).await.unwrap();

        info!("Checking correct execution");
        let wallet_config = fetch_config().await.unwrap();
        let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();
        let account = seq_client
            .get_account(account_id.to_string())
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

    #[nssa_integration_test]
    pub async fn test_pinata_private_receiver() {
        info!("########## test_pinata_private_receiver ##########");
        let pinata_account_id = PINATA_BASE58;
        let pinata_prize = 150;

        let command = Command::Pinata(PinataProgramAgnosticSubcommand::Claim {
            to: make_private_account_input_from_str(ACC_SENDER_PRIVATE),
        });

        let wallet_config = fetch_config().await.unwrap();

        let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

        let pinata_balance_pre = seq_client
            .get_account_balance(pinata_account_id.to_string())
            .await
            .unwrap()
            .balance;

        let SubcommandReturnValue::PrivacyPreservingTransfer { tx_hash: _ } =
            wallet::cli::execute_subcommand(command).await.unwrap()
        else {
            panic!("invalid subcommand return value");
        };

        info!("Waiting for next block creation");
        tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

        info!("Checking correct balance move");
        let pinata_balance_post = seq_client
            .get_account_balance(pinata_account_id.to_string())
            .await
            .unwrap()
            .balance;

        let command = Command::Account(AccountSubcommand::SyncPrivate {});
        wallet::cli::execute_subcommand(command).await.unwrap();

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

    #[nssa_integration_test]
    pub async fn test_pinata_private_receiver_new_account() {
        info!("########## test_pinata_private_receiver_new_account ##########");
        let pinata_account_id = PINATA_BASE58;
        let pinata_prize = 150;

        // Create new account for the token supply holder (private)
        let SubcommandReturnValue::RegisterAccount {
            account_id: winner_account_id,
        } = wallet::cli::execute_subcommand(Command::Account(AccountSubcommand::New(
            NewSubcommand::Private {
                cci: ChainIndex::root(),
            },
        )))
        .await
        .unwrap()
        else {
            panic!("invalid subcommand return value");
        };

        let command = Command::Pinata(PinataProgramAgnosticSubcommand::Claim {
            to: make_private_account_input_from_str(&winner_account_id.to_string()),
        });

        let wallet_config = fetch_config().await.unwrap();

        let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

        let pinata_balance_pre = seq_client
            .get_account_balance(pinata_account_id.to_string())
            .await
            .unwrap()
            .balance;

        wallet::cli::execute_subcommand(command).await.unwrap();

        info!("Waiting for next block creation");
        tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

        info!("Checking correct balance move");
        let pinata_balance_post = seq_client
            .get_account_balance(pinata_account_id.to_string())
            .await
            .unwrap()
            .balance;

        let wallet_config = fetch_config().await.unwrap();
        let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();
        let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config)
            .await
            .unwrap();

        let new_commitment1 = wallet_storage
            .get_private_account_commitment(&winner_account_id)
            .unwrap();
        assert!(verify_commitment_is_in_state(new_commitment1, &seq_client).await);

        assert_eq!(pinata_balance_post, pinata_balance_pre - pinata_prize);

        info!("Success!");
    }

    #[nssa_integration_test]
    pub async fn test_modify_config_fields() {
        info!("########## test_modify_config_fields ##########");

        let wallet_config = fetch_config().await.unwrap();
        let old_seq_poll_timeout_millis = wallet_config.seq_poll_timeout_millis;

        // Change config field
        let command = Command::Config(ConfigSubcommand::Set {
            key: "seq_poll_timeout_millis".to_string(),
            value: "1000".to_string(),
        });
        wallet::cli::execute_subcommand(command).await.unwrap();

        let wallet_config = fetch_config().await.unwrap();

        assert_eq!(wallet_config.seq_poll_timeout_millis, 1000);

        // Return how it was at the beginning
        let command = Command::Config(ConfigSubcommand::Set {
            key: "seq_poll_timeout_millis".to_string(),
            value: old_seq_poll_timeout_millis.to_string(),
        });
        wallet::cli::execute_subcommand(command).await.unwrap();

        info!("Success!");
    }

    #[nssa_integration_test]
    pub async fn test_keys_restoration() {
        info!("########## test_keys_restoration ##########");
        let from: AccountId = ACC_SENDER_PRIVATE.parse().unwrap();

        let command = Command::Account(AccountSubcommand::New(NewSubcommand::Private {
            cci: ChainIndex::root(),
        }));

        let sub_ret = wallet::cli::execute_subcommand(command).await.unwrap();
        let SubcommandReturnValue::RegisterAccount {
            account_id: to_account_id1,
        } = sub_ret
        else {
            panic!("FAILED TO REGISTER ACCOUNT");
        };

        let command = Command::Account(AccountSubcommand::New(NewSubcommand::Private {
            cci: ChainIndex::from_str("/0").unwrap(),
        }));

        let sub_ret = wallet::cli::execute_subcommand(command).await.unwrap();
        let SubcommandReturnValue::RegisterAccount {
            account_id: to_account_id2,
        } = sub_ret
        else {
            panic!("FAILED TO REGISTER ACCOUNT");
        };

        let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
            from: make_private_account_input_from_str(&from.to_string()),
            to: Some(make_private_account_input_from_str(
                &to_account_id1.to_string(),
            )),
            to_npk: None,
            to_ipk: None,
            amount: 100,
        });

        wallet::cli::execute_subcommand(command).await.unwrap();

        let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
            from: make_private_account_input_from_str(&from.to_string()),
            to: Some(make_private_account_input_from_str(
                &to_account_id2.to_string(),
            )),
            to_npk: None,
            to_ipk: None,
            amount: 101,
        });

        wallet::cli::execute_subcommand(command).await.unwrap();

        let from: AccountId = ACC_SENDER.parse().unwrap();

        let command = Command::Account(AccountSubcommand::New(NewSubcommand::Public {
            cci: ChainIndex::root(),
        }));

        let sub_ret = wallet::cli::execute_subcommand(command).await.unwrap();
        let SubcommandReturnValue::RegisterAccount {
            account_id: to_account_id3,
        } = sub_ret
        else {
            panic!("FAILED TO REGISTER ACCOUNT");
        };

        let command = Command::Account(AccountSubcommand::New(NewSubcommand::Public {
            cci: ChainIndex::from_str("/0").unwrap(),
        }));

        let sub_ret = wallet::cli::execute_subcommand(command).await.unwrap();
        let SubcommandReturnValue::RegisterAccount {
            account_id: to_account_id4,
        } = sub_ret
        else {
            panic!("FAILED TO REGISTER ACCOUNT");
        };

        let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
            from: make_public_account_input_from_str(&from.to_string()),
            to: Some(make_public_account_input_from_str(
                &to_account_id3.to_string(),
            )),
            to_npk: None,
            to_ipk: None,
            amount: 102,
        });

        wallet::cli::execute_subcommand(command).await.unwrap();

        let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
            from: make_public_account_input_from_str(&from.to_string()),
            to: Some(make_public_account_input_from_str(
                &to_account_id4.to_string(),
            )),
            to_npk: None,
            to_ipk: None,
            amount: 103,
        });

        wallet::cli::execute_subcommand(command).await.unwrap();

        info!("########## PREPARATION END ##########");

        wallet::cli::execute_keys_restoration("test_pass".to_string(), 10)
            .await
            .unwrap();

        let wallet_config = fetch_config().await.unwrap();
        let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config.clone())
            .await
            .unwrap();

        let acc1 = wallet_storage
            .storage
            .user_data
            .private_key_tree
            .get_node(to_account_id1)
            .expect("Acc 1 should be restored");

        let acc2 = wallet_storage
            .storage
            .user_data
            .private_key_tree
            .get_node(to_account_id2)
            .expect("Acc 2 should be restored");

        let _ = wallet_storage
            .storage
            .user_data
            .public_key_tree
            .get_node(to_account_id3)
            .expect("Acc 3 should be restored");

        let _ = wallet_storage
            .storage
            .user_data
            .public_key_tree
            .get_node(to_account_id4)
            .expect("Acc 4 should be restored");

        assert_eq!(
            acc1.value.1.program_owner,
            Program::authenticated_transfer_program().id()
        );
        assert_eq!(
            acc2.value.1.program_owner,
            Program::authenticated_transfer_program().id()
        );

        assert_eq!(acc1.value.1.balance, 100);
        assert_eq!(acc2.value.1.balance, 101);

        info!("########## TREE CHECKS END ##########");

        let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
            from: make_private_account_input_from_str(&to_account_id1.to_string()),
            to: Some(make_private_account_input_from_str(
                &to_account_id2.to_string(),
            )),
            to_npk: None,
            to_ipk: None,
            amount: 10,
        });

        wallet::cli::execute_subcommand(command).await.unwrap();

        let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
            from: make_public_account_input_from_str(&to_account_id3.to_string()),
            to: Some(make_public_account_input_from_str(
                &to_account_id4.to_string(),
            )),
            to_npk: None,
            to_ipk: None,
            amount: 11,
        });

        wallet::cli::execute_subcommand(command).await.unwrap();

        let wallet_config = fetch_config().await.unwrap();
        let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();
        let wallet_storage = WalletCore::start_from_config_update_chain(wallet_config.clone())
            .await
            .unwrap();

        let comm1 = wallet_storage
            .get_private_account_commitment(&to_account_id1)
            .expect("Acc 1 commitment should exist");
        let comm2 = wallet_storage
            .get_private_account_commitment(&to_account_id2)
            .expect("Acc 2 commitment should exist");

        assert!(verify_commitment_is_in_state(comm1, &seq_client).await);
        assert!(verify_commitment_is_in_state(comm2, &seq_client).await);

        let acc3 = seq_client
            .get_account_balance(to_account_id3.to_string())
            .await
            .expect("Acc 3 must be present in public state");
        let acc4 = seq_client
            .get_account_balance(to_account_id4.to_string())
            .await
            .expect("Acc 4 must be present in public state");

        assert_eq!(acc3.balance, 91);
        assert_eq!(acc4.balance, 114);

        info!("Success!");
    }

    println!("{function_map:#?}");

    function_map
}

#[allow(clippy::type_complexity)]
async fn pre_tps_test(
    test: &TpsTestManager,
) -> Result<(ServerHandle, JoinHandle<Result<()>>, TempDir)> {
    info!("Generating tps test config");
    let mut sequencer_config = test.generate_tps_test_config();
    info!("Done");

    let temp_dir_sequencer = replace_home_dir_with_temp_dir_in_configs(&mut sequencer_config);

    let (seq_http_server_handle, sequencer_loop_handle) =
        startup_sequencer(sequencer_config).await?;

    Ok((
        seq_http_server_handle,
        sequencer_loop_handle,
        temp_dir_sequencer,
    ))
}

pub async fn tps_test() {
    let num_transactions = 300 * 5;
    let target_tps = 12;
    let tps_test = TpsTestManager::new(target_tps, num_transactions);

    let target_time = tps_test.target_time();
    info!("Target time: {:?} seconds", target_time.as_secs());
    let res = pre_tps_test(&tps_test).await.unwrap();

    let wallet_config = fetch_config().await.unwrap();
    let seq_client = SequencerClient::new(wallet_config.sequencer_addr.clone()).unwrap();

    info!("TPS test begin");
    let txs = tps_test.build_public_txs();
    let now = Instant::now();

    let mut tx_hashes = vec![];
    for (i, tx) in txs.into_iter().enumerate() {
        let tx_hash = seq_client.send_tx_public(tx).await.unwrap().tx_hash;
        info!("Sent tx {i}");
        tx_hashes.push(tx_hash);
    }

    for (i, tx_hash) in tx_hashes.iter().enumerate() {
        loop {
            if now.elapsed().as_millis() > target_time.as_millis() {
                panic!("TPS test failed by timeout");
            }

            let tx_obj = seq_client
                .get_transaction_by_hash(tx_hash.clone())
                .await
                .inspect_err(|err| {
                    log::warn!(
                        "Failed to get transaction by hash {tx_hash:#?} with error: {err:#?}"
                    )
                });

            if let Ok(tx_obj) = tx_obj
                && tx_obj.transaction.is_some()
            {
                info!("Found tx {i} with hash {tx_hash}");
                break;
            }
        }
    }
    let time_elapsed = now.elapsed().as_secs();

    info!("TPS test finished successfully");
    info!("Target TPS: {}", target_tps);
    info!(
        "Processed {} transactions in {}s",
        tx_hashes.len(),
        time_elapsed
    );
    info!("Target time: {:?}s", target_time.as_secs());

    post_test(res).await;
}

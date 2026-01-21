use std::time::Duration;

use anyhow::Result;
use integration_tests::{
    ACC_RECEIVER, ACC_SENDER, TIME_TO_WAIT_FOR_BLOCK_SECONDS, TestContext, format_public_account_id,
};
use log::info;
use nssa::program::Program;
use tokio::test;
use wallet::cli::{
    Command, SubcommandReturnValue,
    account::{AccountSubcommand, NewSubcommand},
    programs::native_token_transfer::AuthTransferSubcommand,
};

#[test]
async fn successful_transfer_to_existing_account() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
        from: format_public_account_id(ACC_SENDER),
        to: Some(format_public_account_id(ACC_RECEIVER)),
        to_npk: None,
        to_vpk: None,
        amount: 100,
    });

    wallet::cli::execute_subcommand(ctx.wallet_mut(), command).await?;

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    info!("Checking correct balance move");
    let acc_1_balance = ctx
        .sequencer_client()
        .get_account_balance(ACC_SENDER.to_string())
        .await?;
    let acc_2_balance = ctx
        .sequencer_client()
        .get_account_balance(ACC_RECEIVER.to_string())
        .await?;

    info!("Balance of sender: {acc_1_balance:#?}");
    info!("Balance of receiver: {acc_2_balance:#?}");

    assert_eq!(acc_1_balance.balance, 9900);
    assert_eq!(acc_2_balance.balance, 20100);

    Ok(())
}

#[test]
pub async fn successful_transfer_to_new_account() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    let command = Command::Account(AccountSubcommand::New(NewSubcommand::Public { cci: None }));

    wallet::cli::execute_subcommand(ctx.wallet_mut(), command)
        .await
        .unwrap();

    let new_persistent_account_id = ctx
        .wallet()
        .storage()
        .user_data
        .account_ids()
        .map(ToString::to_string)
        .find(|acc_id| acc_id != ACC_SENDER && acc_id != ACC_RECEIVER)
        .expect("Failed to find newly created account in the wallet storage");

    if new_persistent_account_id == String::new() {
        panic!("Failed to produce new account, not present in persistent accounts");
    }

    let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
        from: format_public_account_id(ACC_SENDER),
        to: Some(format_public_account_id(&new_persistent_account_id)),
        to_npk: None,
        to_vpk: None,
        amount: 100,
    });

    wallet::cli::execute_subcommand(ctx.wallet_mut(), command).await?;

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    info!("Checking correct balance move");
    let acc_1_balance = ctx
        .sequencer_client()
        .get_account_balance(ACC_SENDER.to_string())
        .await?;
    let acc_2_balance = ctx
        .sequencer_client()
        .get_account_balance(new_persistent_account_id)
        .await?;

    info!("Balance of sender: {acc_1_balance:#?}");
    info!("Balance of receiver: {acc_2_balance:#?}");

    assert_eq!(acc_1_balance.balance, 9900);
    assert_eq!(acc_2_balance.balance, 100);

    Ok(())
}

#[test]
async fn failed_transfer_with_insufficient_balance() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
        from: format_public_account_id(ACC_SENDER),
        to: Some(format_public_account_id(ACC_RECEIVER)),
        to_npk: None,
        to_vpk: None,
        amount: 1000000,
    });

    let failed_send = wallet::cli::execute_subcommand(ctx.wallet_mut(), command).await;
    assert!(failed_send.is_err());

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    info!("Checking balances unchanged");
    let acc_1_balance = ctx
        .sequencer_client()
        .get_account_balance(ACC_SENDER.to_string())
        .await?;
    let acc_2_balance = ctx
        .sequencer_client()
        .get_account_balance(ACC_RECEIVER.to_string())
        .await?;

    info!("Balance of sender: {acc_1_balance:#?}");
    info!("Balance of receiver: {acc_2_balance:#?}");

    assert_eq!(acc_1_balance.balance, 10000);
    assert_eq!(acc_2_balance.balance, 20000);

    Ok(())
}

#[test]
async fn two_consecutive_successful_transfers() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    // First transfer
    let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
        from: format_public_account_id(ACC_SENDER),
        to: Some(format_public_account_id(ACC_RECEIVER)),
        to_npk: None,
        to_vpk: None,
        amount: 100,
    });

    wallet::cli::execute_subcommand(ctx.wallet_mut(), command).await?;

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    info!("Checking correct balance move after first transfer");
    let acc_1_balance = ctx
        .sequencer_client()
        .get_account_balance(ACC_SENDER.to_string())
        .await?;
    let acc_2_balance = ctx
        .sequencer_client()
        .get_account_balance(ACC_RECEIVER.to_string())
        .await?;

    info!("Balance of sender: {acc_1_balance:#?}");
    info!("Balance of receiver: {acc_2_balance:#?}");

    assert_eq!(acc_1_balance.balance, 9900);
    assert_eq!(acc_2_balance.balance, 20100);

    info!("First TX Success!");

    // Second transfer
    let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
        from: format_public_account_id(ACC_SENDER),
        to: Some(format_public_account_id(ACC_RECEIVER)),
        to_npk: None,
        to_vpk: None,
        amount: 100,
    });

    wallet::cli::execute_subcommand(ctx.wallet_mut(), command).await?;

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    info!("Checking correct balance move after second transfer");
    let acc_1_balance = ctx
        .sequencer_client()
        .get_account_balance(ACC_SENDER.to_string())
        .await?;
    let acc_2_balance = ctx
        .sequencer_client()
        .get_account_balance(ACC_RECEIVER.to_string())
        .await?;

    info!("Balance of sender: {acc_1_balance:#?}");
    info!("Balance of receiver: {acc_2_balance:#?}");

    assert_eq!(acc_1_balance.balance, 9800);
    assert_eq!(acc_2_balance.balance, 20200);

    info!("Second TX Success!");

    Ok(())
}

#[test]
async fn initialize_public_account() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    let command = Command::Account(AccountSubcommand::New(NewSubcommand::Public { cci: None }));
    let result = wallet::cli::execute_subcommand(ctx.wallet_mut(), command).await?;
    let SubcommandReturnValue::RegisterAccount { account_id } = result else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    let command = Command::AuthTransfer(AuthTransferSubcommand::Init {
        account_id: format_public_account_id(&account_id.to_string()),
    });
    wallet::cli::execute_subcommand(ctx.wallet_mut(), command).await?;

    info!("Checking correct execution");
    let account = ctx
        .sequencer_client()
        .get_account(account_id.to_string())
        .await?
        .account;

    assert_eq!(
        account.program_owner,
        Program::authenticated_transfer_program().id()
    );
    assert_eq!(account.balance, 0);
    assert_eq!(account.nonce, 1);
    assert!(account.data.is_empty());

    info!("Successfully initialized public account");

    Ok(())
}

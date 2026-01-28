use std::time::Duration;

use anyhow::{Context as _, Result};
use integration_tests::{
    TIME_TO_WAIT_FOR_BLOCK_SECONDS, TestContext, format_private_account_id,
    format_public_account_id, verify_commitment_is_in_state,
};
use key_protocol::key_management::key_tree::chain_index::ChainIndex;
use log::info;
use nssa::program::Program;
use tokio::test;
use wallet::cli::{
    Command, SubcommandReturnValue,
    account::{AccountSubcommand, NewSubcommand},
    programs::token::TokenProgramAgnosticSubcommand,
};

#[test]
async fn create_and_transfer_public_token() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    // Create new account for the token definition
    let result = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Public { cci: None })),
    )
    .await?;
    let SubcommandReturnValue::RegisterAccount {
        account_id: definition_account_id,
    } = result
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create new account for the token supply holder
    let result = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Public { cci: None })),
    )
    .await?;
    let SubcommandReturnValue::RegisterAccount {
        account_id: supply_account_id,
    } = result
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create new account for receiving a token transaction
    let result = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Public { cci: None })),
    )
    .await?;
    let SubcommandReturnValue::RegisterAccount {
        account_id: recipient_account_id,
    } = result
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create new token
    let subcommand = TokenProgramAgnosticSubcommand::New {
        definition_account_id: format_public_account_id(&definition_account_id.to_string()),
        supply_account_id: format_public_account_id(&supply_account_id.to_string()),
        name: "A NAME".to_string(),
        total_supply: 37,
    };
    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::Token(subcommand)).await?;

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    // Check the status of the token definition account
    let definition_acc = ctx
        .sequencer_client()
        .get_account(definition_account_id.to_string())
        .await?
        .account;

    assert_eq!(definition_acc.program_owner, Program::token().id());
    // The data of a token definition account has the following layout:
    // [ 0x00 || name (6 bytes) || total supply (little endian 16 bytes) || metadata id (32 bytes)]
    assert_eq!(
        definition_acc.data.as_ref(),
        &[
            0, 65, 32, 78, 65, 77, 69, 37, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
        ]
    );

    // Check the status of the token holding account with the total supply
    let supply_acc = ctx
        .sequencer_client()
        .get_account(supply_account_id.to_string())
        .await?
        .account;

    // The account must be owned by the token program
    assert_eq!(supply_acc.program_owner, Program::token().id());
    // The data of a token holding account has the following layout:
    // [ 0x01 || corresponding_token_definition_id (32 bytes) || balance (little endian 16 bytes) ]
    // First byte of the data equal to 1 means it's a token holding account
    assert_eq!(supply_acc.data.as_ref()[0], 1);
    // Bytes from 1 to 33 represent the id of the token this account is associated with
    assert_eq!(
        &supply_acc.data.as_ref()[1..33],
        definition_account_id.to_bytes()
    );
    assert_eq!(u128::from_le_bytes(supply_acc.data[33..].try_into()?), 37);

    // Transfer 7 tokens from supply_acc to recipient_account_id
    let subcommand = TokenProgramAgnosticSubcommand::Send {
        from: format_public_account_id(&supply_account_id.to_string()),
        to: Some(format_public_account_id(&recipient_account_id.to_string())),
        to_npk: None,
        to_vpk: None,
        amount: 7,
    };

    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::Token(subcommand)).await?;

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    // Check the status of the supply account after transfer
    let supply_acc = ctx
        .sequencer_client()
        .get_account(supply_account_id.to_string())
        .await?
        .account;
    assert_eq!(supply_acc.program_owner, Program::token().id());
    assert_eq!(supply_acc.data[0], 1);
    assert_eq!(&supply_acc.data[1..33], definition_account_id.to_bytes());
    assert_eq!(u128::from_le_bytes(supply_acc.data[33..].try_into()?), 30);

    // Check the status of the recipient account after transfer
    let recipient_acc = ctx
        .sequencer_client()
        .get_account(recipient_account_id.to_string())
        .await?
        .account;
    assert_eq!(recipient_acc.program_owner, Program::token().id());
    assert_eq!(recipient_acc.data[0], 1);
    assert_eq!(&recipient_acc.data[1..33], definition_account_id.to_bytes());
    assert_eq!(u128::from_le_bytes(recipient_acc.data[33..].try_into()?), 7);

    // Burn 3 tokens from recipient_acc
    let subcommand = TokenProgramAgnosticSubcommand::Burn {
        definition: format_public_account_id(&definition_account_id.to_string()),
        holder: format_public_account_id(&recipient_account_id.to_string()),
        amount: 3,
    };

    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::Token(subcommand)).await?;

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    // Check the status of the token definition account after burn
    let definition_acc = ctx
        .sequencer_client()
        .get_account(definition_account_id.to_string())
        .await?
        .account;

    assert_eq!(
        definition_acc.data.as_ref(),
        &[
            0, 65, 32, 78, 65, 77, 69, 34, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
        ]
    );

    // Check the status of the recipient account after burn
    let recipient_acc = ctx
        .sequencer_client()
        .get_account(recipient_account_id.to_string())
        .await?
        .account;

    assert_eq!(u128::from_le_bytes(recipient_acc.data[33..].try_into()?), 4);

    // Mint 10 tokens at recipient_acc
    let subcommand = TokenProgramAgnosticSubcommand::Mint {
        definition: format_public_account_id(&definition_account_id.to_string()),
        holder: Some(format_public_account_id(&recipient_account_id.to_string())),
        holder_npk: None,
        holder_vpk: None,
        amount: 10,
    };

    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::Token(subcommand)).await?;

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    // Check the status of the token definition account after mint
    let definition_acc = ctx
        .sequencer_client()
        .get_account(definition_account_id.to_string())
        .await?
        .account;

    assert_eq!(
        definition_acc.data.as_ref(),
        &[
            0, 65, 32, 78, 65, 77, 69, 44, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
        ]
    );

    // Check the status of the recipient account after mint
    let recipient_acc = ctx
        .sequencer_client()
        .get_account(recipient_account_id.to_string())
        .await?
        .account;

    assert_eq!(
        u128::from_le_bytes(recipient_acc.data[33..].try_into()?),
        14
    );

    info!("Successfully created and transferred public token");

    Ok(())
}

#[test]
async fn create_and_transfer_token_with_private_supply() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    // Create new account for the token definition (public)
    let result = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Public { cci: None })),
    )
    .await?;
    let SubcommandReturnValue::RegisterAccount {
        account_id: definition_account_id,
    } = result
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create new account for the token supply holder (private)
    let result = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Private { cci: None })),
    )
    .await?;
    let SubcommandReturnValue::RegisterAccount {
        account_id: supply_account_id,
    } = result
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create new account for receiving a token transaction (private)
    let result = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Private { cci: None })),
    )
    .await?;
    let SubcommandReturnValue::RegisterAccount {
        account_id: recipient_account_id,
    } = result
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create new token
    let subcommand = TokenProgramAgnosticSubcommand::New {
        definition_account_id: format_public_account_id(&definition_account_id.to_string()),
        supply_account_id: format_private_account_id(&supply_account_id.to_string()),
        name: "A NAME".to_string(),
        total_supply: 37,
    };

    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::Token(subcommand)).await?;

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    // Check the status of the token definition account
    let definition_acc = ctx
        .sequencer_client()
        .get_account(definition_account_id.to_string())
        .await?
        .account;

    assert_eq!(definition_acc.program_owner, Program::token().id());
    assert_eq!(
        definition_acc.data.as_ref(),
        &[
            0, 65, 32, 78, 65, 77, 69, 37, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
        ]
    );

    let new_commitment1 = ctx
        .wallet()
        .get_private_account_commitment(&supply_account_id)
        .context("Failed to get supply account commitment")?;
    assert!(verify_commitment_is_in_state(new_commitment1, ctx.sequencer_client()).await);

    // Transfer 7 tokens from supply_acc to recipient_account_id
    let subcommand = TokenProgramAgnosticSubcommand::Send {
        from: format_private_account_id(&supply_account_id.to_string()),
        to: Some(format_private_account_id(&recipient_account_id.to_string())),
        to_npk: None,
        to_vpk: None,
        amount: 7,
    };

    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::Token(subcommand)).await?;

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    let new_commitment1 = ctx
        .wallet()
        .get_private_account_commitment(&supply_account_id)
        .context("Failed to get supply account commitment")?;
    assert!(verify_commitment_is_in_state(new_commitment1, ctx.sequencer_client()).await);

    let new_commitment2 = ctx
        .wallet()
        .get_private_account_commitment(&recipient_account_id)
        .context("Failed to get recipient account commitment")?;
    assert!(verify_commitment_is_in_state(new_commitment2, ctx.sequencer_client()).await);

    // Burn 3 tokens from recipient_acc
    let subcommand = TokenProgramAgnosticSubcommand::Burn {
        definition: format_public_account_id(&definition_account_id.to_string()),
        holder: format_private_account_id(&recipient_account_id.to_string()),
        amount: 3,
    };

    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::Token(subcommand)).await?;

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    // Check the token definition account after burn
    let definition_acc = ctx
        .sequencer_client()
        .get_account(definition_account_id.to_string())
        .await?
        .account;

    assert_eq!(
        definition_acc.data.as_ref(),
        &[
            0, 65, 32, 78, 65, 77, 69, 34, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
        ]
    );

    let new_commitment2 = ctx
        .wallet()
        .get_private_account_commitment(&recipient_account_id)
        .context("Failed to get recipient account commitment")?;
    assert!(verify_commitment_is_in_state(new_commitment2, ctx.sequencer_client()).await);

    // Check the recipient account balance after burn
    let recipient_acc = ctx
        .wallet()
        .get_account_private(&recipient_account_id)
        .context("Failed to get recipient account")?;

    assert_eq!(
        u128::from_le_bytes(recipient_acc.data[33..].try_into()?),
        4 // 7 - 3
    );

    info!("Successfully created and transferred token with private supply");

    Ok(())
}

#[test]
async fn create_token_with_private_definition() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    // Create token definition account (private)
    let result = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Private {
            cci: Some(ChainIndex::root()),
        })),
    )
    .await?;
    let SubcommandReturnValue::RegisterAccount {
        account_id: definition_account_id,
    } = result
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create supply account (public)
    let result = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Public {
            cci: Some(ChainIndex::root()),
        })),
    )
    .await?;
    let SubcommandReturnValue::RegisterAccount {
        account_id: supply_account_id,
    } = result
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create token with private definition
    let subcommand = TokenProgramAgnosticSubcommand::New {
        definition_account_id: format_private_account_id(&definition_account_id.to_string()),
        supply_account_id: format_public_account_id(&supply_account_id.to_string()),
        name: "A NAME".to_string(),
        total_supply: 37,
    };

    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::Token(subcommand)).await?;

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    // Verify private definition commitment
    let new_commitment = ctx
        .wallet()
        .get_private_account_commitment(&definition_account_id)
        .context("Failed to get definition commitment")?;
    assert!(verify_commitment_is_in_state(new_commitment, ctx.sequencer_client()).await);

    // Verify supply account
    let supply_acc = ctx
        .sequencer_client()
        .get_account(supply_account_id.to_string())
        .await?
        .account;

    assert_eq!(supply_acc.program_owner, Program::token().id());
    assert_eq!(supply_acc.data.as_ref()[0], 1);
    assert_eq!(u128::from_le_bytes(supply_acc.data[33..].try_into()?), 37);

    // Create private recipient account
    let result = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Private { cci: None })),
    )
    .await?;
    let SubcommandReturnValue::RegisterAccount {
        account_id: recipient_account_id_private,
    } = result
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create public recipient account
    let result = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Public { cci: None })),
    )
    .await?;
    let SubcommandReturnValue::RegisterAccount {
        account_id: recipient_account_id_public,
    } = result
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Mint to public account
    let subcommand = TokenProgramAgnosticSubcommand::Mint {
        definition: format_private_account_id(&definition_account_id.to_string()),
        holder: Some(format_public_account_id(
            &recipient_account_id_public.to_string(),
        )),
        holder_npk: None,
        holder_vpk: None,
        amount: 10,
    };

    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::Token(subcommand)).await?;

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    // Verify definition account has updated supply
    let definition_acc = ctx
        .wallet()
        .get_account_private(&definition_account_id)
        .context("Failed to get definition account")?;

    assert_eq!(
        u128::from_le_bytes(definition_acc.data[7..23].try_into()?),
        47 // 37 + 10
    );

    // Verify public recipient received tokens
    let recipient_acc = ctx
        .sequencer_client()
        .get_account(recipient_account_id_public.to_string())
        .await?
        .account;

    assert_eq!(
        u128::from_le_bytes(recipient_acc.data[33..].try_into()?),
        10
    );

    // Mint to private account
    let subcommand = TokenProgramAgnosticSubcommand::Mint {
        definition: format_private_account_id(&definition_account_id.to_string()),
        holder: Some(format_private_account_id(
            &recipient_account_id_private.to_string(),
        )),
        holder_npk: None,
        holder_vpk: None,
        amount: 5,
    };

    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::Token(subcommand)).await?;

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    // Verify private recipient commitment
    let new_commitment = ctx
        .wallet()
        .get_private_account_commitment(&recipient_account_id_private)
        .context("Failed to get recipient commitment")?;
    assert!(verify_commitment_is_in_state(new_commitment, ctx.sequencer_client()).await);

    // Verify private recipient balance
    let recipient_acc_private = ctx
        .wallet()
        .get_account_private(&recipient_account_id_private)
        .context("Failed to get private recipient account")?;

    assert_eq!(
        u128::from_le_bytes(recipient_acc_private.data[33..].try_into()?),
        5
    );

    info!("Successfully created token with private definition and minted to both account types");

    Ok(())
}

#[test]
async fn create_token_with_private_definition_and_supply() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    // Create token definition account (private)
    let result = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Private { cci: None })),
    )
    .await?;
    let SubcommandReturnValue::RegisterAccount {
        account_id: definition_account_id,
    } = result
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create supply account (private)
    let result = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Private { cci: None })),
    )
    .await?;
    let SubcommandReturnValue::RegisterAccount {
        account_id: supply_account_id,
    } = result
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create token with both private definition and supply
    let subcommand = TokenProgramAgnosticSubcommand::New {
        definition_account_id: format_private_account_id(&definition_account_id.to_string()),
        supply_account_id: format_private_account_id(&supply_account_id.to_string()),
        name: "A NAME".to_string(),
        total_supply: 37,
    };

    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::Token(subcommand)).await?;

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    // Verify definition commitment
    let definition_commitment = ctx
        .wallet()
        .get_private_account_commitment(&definition_account_id)
        .context("Failed to get definition commitment")?;
    assert!(verify_commitment_is_in_state(definition_commitment, ctx.sequencer_client()).await);

    // Verify supply commitment
    let supply_commitment = ctx
        .wallet()
        .get_private_account_commitment(&supply_account_id)
        .context("Failed to get supply commitment")?;
    assert!(verify_commitment_is_in_state(supply_commitment, ctx.sequencer_client()).await);

    // Verify supply balance
    let supply_acc = ctx
        .wallet()
        .get_account_private(&supply_account_id)
        .context("Failed to get supply account")?;

    assert_eq!(u128::from_le_bytes(supply_acc.data[33..].try_into()?), 37);

    // Create recipient account
    let result = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Private { cci: None })),
    )
    .await?;
    let SubcommandReturnValue::RegisterAccount {
        account_id: recipient_account_id,
    } = result
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Transfer tokens
    let subcommand = TokenProgramAgnosticSubcommand::Send {
        from: format_private_account_id(&supply_account_id.to_string()),
        to: Some(format_private_account_id(&recipient_account_id.to_string())),
        to_npk: None,
        to_vpk: None,
        amount: 7,
    };

    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::Token(subcommand)).await?;

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    // Verify both commitments updated
    let supply_commitment = ctx
        .wallet()
        .get_private_account_commitment(&supply_account_id)
        .context("Failed to get supply commitment")?;
    assert!(verify_commitment_is_in_state(supply_commitment, ctx.sequencer_client()).await);

    let recipient_commitment = ctx
        .wallet()
        .get_private_account_commitment(&recipient_account_id)
        .context("Failed to get recipient commitment")?;
    assert!(verify_commitment_is_in_state(recipient_commitment, ctx.sequencer_client()).await);

    // Verify balances
    let supply_acc = ctx
        .wallet()
        .get_account_private(&supply_account_id)
        .context("Failed to get supply account")?;
    assert_eq!(u128::from_le_bytes(supply_acc.data[33..].try_into()?), 30);

    let recipient_acc = ctx
        .wallet()
        .get_account_private(&recipient_account_id)
        .context("Failed to get recipient account")?;
    assert_eq!(u128::from_le_bytes(recipient_acc.data[33..].try_into()?), 7);

    info!("Successfully created and transferred token with both private definition and supply");

    Ok(())
}

#[test]
async fn shielded_token_transfer() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    // Create token definition account (public)
    let result = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Public { cci: None })),
    )
    .await?;
    let SubcommandReturnValue::RegisterAccount {
        account_id: definition_account_id,
    } = result
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create supply account (public)
    let result = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Public { cci: None })),
    )
    .await?;
    let SubcommandReturnValue::RegisterAccount {
        account_id: supply_account_id,
    } = result
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create recipient account (private) for shielded transfer
    let result = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Private { cci: None })),
    )
    .await?;
    let SubcommandReturnValue::RegisterAccount {
        account_id: recipient_account_id,
    } = result
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create token
    let subcommand = TokenProgramAgnosticSubcommand::New {
        definition_account_id: format_public_account_id(&definition_account_id.to_string()),
        supply_account_id: format_public_account_id(&supply_account_id.to_string()),
        name: "A NAME".to_string(),
        total_supply: 37,
    };

    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::Token(subcommand)).await?;

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    // Perform shielded transfer: public supply -> private recipient
    let subcommand = TokenProgramAgnosticSubcommand::Send {
        from: format_public_account_id(&supply_account_id.to_string()),
        to: Some(format_private_account_id(&recipient_account_id.to_string())),
        to_npk: None,
        to_vpk: None,
        amount: 7,
    };

    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::Token(subcommand)).await?;

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    // Verify supply account balance
    let supply_acc = ctx
        .sequencer_client()
        .get_account(supply_account_id.to_string())
        .await?
        .account;
    assert_eq!(u128::from_le_bytes(supply_acc.data[33..].try_into()?), 30);

    // Verify recipient commitment exists
    let new_commitment = ctx
        .wallet()
        .get_private_account_commitment(&recipient_account_id)
        .context("Failed to get recipient commitment")?;
    assert!(verify_commitment_is_in_state(new_commitment, ctx.sequencer_client()).await);

    // Verify recipient balance
    let recipient_acc = ctx
        .wallet()
        .get_account_private(&recipient_account_id)
        .context("Failed to get recipient account")?;
    assert_eq!(u128::from_le_bytes(recipient_acc.data[33..].try_into()?), 7);

    info!("Successfully performed shielded token transfer");

    Ok(())
}

#[test]
async fn deshielded_token_transfer() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    // Create token definition account (public)
    let result = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Public { cci: None })),
    )
    .await?;
    let SubcommandReturnValue::RegisterAccount {
        account_id: definition_account_id,
    } = result
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create supply account (private)
    let result = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Private { cci: None })),
    )
    .await?;
    let SubcommandReturnValue::RegisterAccount {
        account_id: supply_account_id,
    } = result
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create recipient account (public) for deshielded transfer
    let result = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Public { cci: None })),
    )
    .await?;
    let SubcommandReturnValue::RegisterAccount {
        account_id: recipient_account_id,
    } = result
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create token with private supply
    let subcommand = TokenProgramAgnosticSubcommand::New {
        definition_account_id: format_public_account_id(&definition_account_id.to_string()),
        supply_account_id: format_private_account_id(&supply_account_id.to_string()),
        name: "A NAME".to_string(),
        total_supply: 37,
    };

    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::Token(subcommand)).await?;

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    // Perform deshielded transfer: private supply -> public recipient
    let subcommand = TokenProgramAgnosticSubcommand::Send {
        from: format_private_account_id(&supply_account_id.to_string()),
        to: Some(format_public_account_id(&recipient_account_id.to_string())),
        to_npk: None,
        to_vpk: None,
        amount: 7,
    };

    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::Token(subcommand)).await?;

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    // Verify supply account commitment exists
    let new_commitment = ctx
        .wallet()
        .get_private_account_commitment(&supply_account_id)
        .context("Failed to get supply commitment")?;
    assert!(verify_commitment_is_in_state(new_commitment, ctx.sequencer_client()).await);

    // Verify supply balance
    let supply_acc = ctx
        .wallet()
        .get_account_private(&supply_account_id)
        .context("Failed to get supply account")?;
    assert_eq!(u128::from_le_bytes(supply_acc.data[33..].try_into()?), 30);

    // Verify recipient balance
    let recipient_acc = ctx
        .sequencer_client()
        .get_account(recipient_account_id.to_string())
        .await?
        .account;
    assert_eq!(u128::from_le_bytes(recipient_acc.data[33..].try_into()?), 7);

    info!("Successfully performed deshielded token transfer");

    Ok(())
}

#[test]
async fn token_claiming_path_with_private_accounts() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    // Create token definition account (private)
    let result = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Private { cci: None })),
    )
    .await?;
    let SubcommandReturnValue::RegisterAccount {
        account_id: definition_account_id,
    } = result
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create supply account (private)
    let result = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Private { cci: None })),
    )
    .await?;
    let SubcommandReturnValue::RegisterAccount {
        account_id: supply_account_id,
    } = result
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create token
    let subcommand = TokenProgramAgnosticSubcommand::New {
        definition_account_id: format_private_account_id(&definition_account_id.to_string()),
        supply_account_id: format_private_account_id(&supply_account_id.to_string()),
        name: "A NAME".to_string(),
        total_supply: 37,
    };

    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::Token(subcommand)).await?;

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    // Create new private account for claiming path
    let result = wallet::cli::execute_subcommand(
        ctx.wallet_mut(),
        Command::Account(AccountSubcommand::New(NewSubcommand::Private { cci: None })),
    )
    .await?;
    let SubcommandReturnValue::RegisterAccount {
        account_id: recipient_account_id,
    } = result
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Get keys for foreign mint (claiming path)
    let (holder_keys, _) = ctx
        .wallet()
        .storage()
        .user_data
        .get_private_account(&recipient_account_id)
        .cloned()
        .context("Failed to get private account keys")?;

    // Mint using claiming path (foreign account)
    let subcommand = TokenProgramAgnosticSubcommand::Mint {
        definition: format_private_account_id(&definition_account_id.to_string()),
        holder: None,
        holder_npk: Some(hex::encode(holder_keys.nullifer_public_key.0)),
        holder_vpk: Some(hex::encode(holder_keys.viewing_public_key.0)),
        amount: 9,
    };

    wallet::cli::execute_subcommand(ctx.wallet_mut(), Command::Token(subcommand)).await?;

    info!("Waiting for next block creation");
    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    // Sync to claim the account
    let command = Command::Account(AccountSubcommand::SyncPrivate {});
    wallet::cli::execute_subcommand(ctx.wallet_mut(), command).await?;

    // Verify commitment exists
    let recipient_commitment = ctx
        .wallet()
        .get_private_account_commitment(&recipient_account_id)
        .context("Failed to get recipient commitment")?;
    assert!(verify_commitment_is_in_state(recipient_commitment, ctx.sequencer_client()).await);

    // Verify balance
    let recipient_acc = ctx
        .wallet()
        .get_account_private(&recipient_account_id)
        .context("Failed to get recipient account")?;
    assert_eq!(u128::from_le_bytes(recipient_acc.data[33..].try_into()?), 9);

    info!("Successfully minted tokens using claiming path");

    Ok(())
}

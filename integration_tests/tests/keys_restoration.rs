use std::{str::FromStr, time::Duration};

use anyhow::Result;
use integration_tests::{
    TIME_TO_WAIT_FOR_BLOCK_SECONDS, TestContext, format_private_account_id,
    format_public_account_id, verify_commitment_is_in_state,
};
use key_protocol::key_management::key_tree::chain_index::ChainIndex;
use log::info;
use nssa::{AccountId, program::Program};
use tokio::test;
use wallet::cli::{
    Command, SubcommandReturnValue,
    account::{AccountSubcommand, NewSubcommand},
    programs::native_token_transfer::AuthTransferSubcommand,
};

#[test]
async fn restore_keys_from_seed() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    let from: AccountId = ctx.existing_private_accounts()[0];

    // Create first private account at root
    let command = Command::Account(AccountSubcommand::New(NewSubcommand::Private {
        cci: Some(ChainIndex::root()),
    }));
    let result = wallet::cli::execute_subcommand(ctx.wallet_mut(), command).await?;
    let SubcommandReturnValue::RegisterAccount {
        account_id: to_account_id1,
    } = result
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create second private account at /0
    let command = Command::Account(AccountSubcommand::New(NewSubcommand::Private {
        cci: Some(ChainIndex::from_str("/0")?),
    }));
    let result = wallet::cli::execute_subcommand(ctx.wallet_mut(), command).await?;
    let SubcommandReturnValue::RegisterAccount {
        account_id: to_account_id2,
    } = result
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Send to first private account
    let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
        from: format_private_account_id(from),
        to: Some(format_private_account_id(to_account_id1)),
        to_npk: None,
        to_ipk: None,
        amount: 100,
    });
    wallet::cli::execute_subcommand(ctx.wallet_mut(), command).await?;

    // Send to second private account
    let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
        from: format_private_account_id(from),
        to: Some(format_private_account_id(to_account_id2)),
        to_npk: None,
        to_ipk: None,
        amount: 101,
    });
    wallet::cli::execute_subcommand(ctx.wallet_mut(), command).await?;

    let from: AccountId = ctx.existing_public_accounts()[0];

    // Create first public account at root
    let command = Command::Account(AccountSubcommand::New(NewSubcommand::Public {
        cci: Some(ChainIndex::root()),
    }));
    let result = wallet::cli::execute_subcommand(ctx.wallet_mut(), command).await?;
    let SubcommandReturnValue::RegisterAccount {
        account_id: to_account_id3,
    } = result
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Create second public account at /0
    let command = Command::Account(AccountSubcommand::New(NewSubcommand::Public {
        cci: Some(ChainIndex::from_str("/0")?),
    }));
    let result = wallet::cli::execute_subcommand(ctx.wallet_mut(), command).await?;
    let SubcommandReturnValue::RegisterAccount {
        account_id: to_account_id4,
    } = result
    else {
        anyhow::bail!("Expected RegisterAccount return value");
    };

    // Send to first public account
    let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
        from: format_public_account_id(from),
        to: Some(format_public_account_id(to_account_id3)),
        to_npk: None,
        to_ipk: None,
        amount: 102,
    });
    wallet::cli::execute_subcommand(ctx.wallet_mut(), command).await?;

    // Send to second public account
    let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
        from: format_public_account_id(from),
        to: Some(format_public_account_id(to_account_id4)),
        to_npk: None,
        to_ipk: None,
        amount: 103,
    });
    wallet::cli::execute_subcommand(ctx.wallet_mut(), command).await?;

    info!("Preparation complete, performing keys restoration");

    // Restore keys from seed
    wallet::cli::execute_keys_restoration(ctx.wallet_mut(), 10).await?;

    // Verify restored private accounts
    let acc1 = ctx
        .wallet()
        .storage()
        .user_data
        .private_key_tree
        .get_node(to_account_id1)
        .expect("Acc 1 should be restored");

    let acc2 = ctx
        .wallet()
        .storage()
        .user_data
        .private_key_tree
        .get_node(to_account_id2)
        .expect("Acc 2 should be restored");

    // Verify restored public accounts
    let _acc3 = ctx
        .wallet()
        .storage()
        .user_data
        .public_key_tree
        .get_node(to_account_id3)
        .expect("Acc 3 should be restored");

    let _acc4 = ctx
        .wallet()
        .storage()
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

    info!("Tree checks passed, testing restored accounts can transact");

    // Test that restored accounts can send transactions
    let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
        from: format_private_account_id(to_account_id1),
        to: Some(format_private_account_id(to_account_id2)),
        to_npk: None,
        to_ipk: None,
        amount: 10,
    });
    wallet::cli::execute_subcommand(ctx.wallet_mut(), command).await?;

    let command = Command::AuthTransfer(AuthTransferSubcommand::Send {
        from: format_public_account_id(to_account_id3),
        to: Some(format_public_account_id(to_account_id4)),
        to_npk: None,
        to_ipk: None,
        amount: 11,
    });
    wallet::cli::execute_subcommand(ctx.wallet_mut(), command).await?;

    tokio::time::sleep(Duration::from_secs(TIME_TO_WAIT_FOR_BLOCK_SECONDS)).await;

    // Verify commitments exist for private accounts
    let comm1 = ctx
        .wallet()
        .get_private_account_commitment(to_account_id1)
        .expect("Acc 1 commitment should exist");
    let comm2 = ctx
        .wallet()
        .get_private_account_commitment(to_account_id2)
        .expect("Acc 2 commitment should exist");

    assert!(verify_commitment_is_in_state(comm1, ctx.sequencer_client()).await);
    assert!(verify_commitment_is_in_state(comm2, ctx.sequencer_client()).await);

    // Verify public account balances
    let acc3 = ctx
        .sequencer_client()
        .get_account_balance(to_account_id3)
        .await?;
    let acc4 = ctx
        .sequencer_client()
        .get_account_balance(to_account_id4)
        .await?;

    assert_eq!(acc3.balance, 91); // 102 - 11
    assert_eq!(acc4.balance, 114); // 103 + 11

    info!("Successfully restored keys and verified transactions");

    Ok(())
}

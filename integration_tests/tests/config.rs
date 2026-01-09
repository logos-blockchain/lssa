use anyhow::Result;
use integration_tests::TestContext;
use log::info;
use tokio::test;
use wallet::cli::{Command, config::ConfigSubcommand};

#[test]
async fn modify_config_field() -> Result<()> {
    let mut ctx = TestContext::new().await?;

    let old_seq_poll_timeout_millis = ctx.wallet().config().seq_poll_timeout_millis;

    // Change config field
    let command = Command::Config(ConfigSubcommand::Set {
        key: "seq_poll_timeout_millis".to_string(),
        value: "1000".to_string(),
    });
    wallet::cli::execute_subcommand(ctx.wallet_mut(), command).await?;

    let new_seq_poll_timeout_millis = ctx.wallet().config().seq_poll_timeout_millis;
    assert_eq!(new_seq_poll_timeout_millis, 1000);

    // Return how it was at the beginning
    let command = Command::Config(ConfigSubcommand::Set {
        key: "seq_poll_timeout_millis".to_string(),
        value: old_seq_poll_timeout_millis.to_string(),
    });
    wallet::cli::execute_subcommand(ctx.wallet_mut(), command).await?;

    info!("Successfully modified and restored config field");

    Ok(())
}

use anyhow::Result;
use integration_tests::{ACC_SENDER, TestContext};
use log::info;
use nssa::program::Program;
use tokio::test;

#[test]
async fn get_existing_account() -> Result<()> {
    let ctx = TestContext::new().await?;

    let account = ctx
        .sequencer_client()
        .get_account(ACC_SENDER.to_string())
        .await?
        .account;

    assert_eq!(
        account.program_owner,
        Program::authenticated_transfer_program().id()
    );
    assert_eq!(account.balance, 10000);
    assert!(account.data.is_empty());
    assert_eq!(account.nonce, 0);

    info!("Successfully retrieved account with correct details");

    Ok(())
}

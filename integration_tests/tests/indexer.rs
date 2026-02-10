use anyhow::Result;
use indexer_service_rpc::RpcClient;
use integration_tests::TestContext;
use log::info;
use tokio::test;
// use wallet::cli::{Command, config::ConfigSubcommand};

#[test]
async fn indexer_test_run() -> Result<()> {
    let ctx = TestContext::new().await?;

    // RUN OBSERVATION
    info!("LETS TAKE A LOOK");
    tokio::time::sleep(std::time::Duration::from_secs(100)).await;

    let last_block_seq = ctx
        .sequencer_client()
        .get_last_block()
        .await
        .unwrap()
        .last_block;

    info!("Last block on seq now is {last_block_seq}");

    let last_block_indexer = ctx
        .indexer_client()
        .get_last_finalized_block_id()
        .await
        .unwrap();

    info!("Last block on ind now is {last_block_indexer}");

    assert!(last_block_indexer > 1);

    Ok(())
}

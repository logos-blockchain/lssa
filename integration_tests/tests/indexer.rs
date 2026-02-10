use anyhow::Result;
use integration_tests::TestContext;
use log::info;
use tokio::test;

#[ignore = "needs complicated setup"]
#[test]
// To run this test properly, you need nomos node running in the background.
// For instructions in building nomos node, refer to [this](https://github.com/logos-blockchain/logos-blockchain?tab=readme-ov-file#running-a-logos-blockchain-node).
//
// Recommended to run node locally from build binary.
async fn indexer_run_local_node() -> Result<()> {
    let _ctx = TestContext::new_bedrock_local_attached().await?;

    info!("Let's observe behaviour");

    tokio::time::sleep(std::time::Duration::from_secs(180)).await;

    // No way to check state of indexer now
    // When it will be a service, then it will become possible.

    Ok(())
}

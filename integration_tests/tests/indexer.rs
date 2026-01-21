use anyhow::Result;
use integration_tests::TestContext;
use log::info;
use tokio::test;

#[ignore = "needs complicated setup"]
#[test]
/// To run this test properly, you need nomos node running in the background.
/// For instructions in building nomos node, refer to [this](https://github.com/logos-blockchain/logos-blockchain?tab=readme-ov-file#running-a-logos-blockchain-node).
///
/// Recommended to run node locally from build binary.
async fn indexer_run_local_node() -> Result<()> {
    let ctx = TestContext::new_bedrock_local_attached().await?;

    info!("Let's observe behaviour");

    tokio::time::sleep(std::time::Duration::from_secs(180)).await;

    let gen_id = ctx
        .sequencer_client()
        .get_last_seen_l2_block_at_indexer()
        .await
        .unwrap()
        .last_block
        .unwrap();

    // Checking, that some blocks are landed on bedrock
    assert!(gen_id > 0);

    info!("Last seen L2 block at indexer is {gen_id}");

    Ok(())
}

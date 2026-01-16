use anyhow::Result;
use integration_tests::TestContext;
use log::info;
use tokio::test;

#[test]
async fn indexer_run_local_node() -> Result<()> {
    println!("Waiting 20 seconds for L1 node to start producing");
    tokio::time::sleep(std::time::Duration::from_secs(30)).await;

    let ctx = TestContext::new_bedrock_local_attached().await?;

    info!("Let's observe behaviour");

    tokio::time::sleep(std::time::Duration::from_secs(600)).await;

    let gen_id = ctx
        .sequencer_client()
        .get_last_seen_l2_block_at_indexer()
        .await
        .unwrap();

    info!("Last seen L2 block at indexer is {}", gen_id.last_block);

    Ok(())
}

use anyhow::Result;
use integration_tests::TestContext;
use log::info;
use tokio::test;

#[test]
async fn indexer_run_local_node() -> Result<()> {
    println!("Waiting 20 seconds for L1 node to start producing");
    tokio::time::sleep(std::time::Duration::from_secs(20)).await;

    let ctx = TestContext::new().await?;

    info!("Let's observe behaviour");

    tokio::time::sleep(std::time::Duration::from_secs(30)).await;

    let gen_id = ctx.sequencer_client().get_genesis_id().await.unwrap();

    info!("btw, gen id is {gen_id:?}");

    Ok(())
}
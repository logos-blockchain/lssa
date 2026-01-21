use anyhow::Result;
use broadcast_service::BlockInfo;
use common_http_client::CommonHttpClient;
pub use common_http_client::{BasicAuthCredentials, Error};
use futures::{Stream, TryFutureExt};
use log::warn;
use nomos_core::{block::Block, header::HeaderId, mantle::SignedMantleTx};
use reqwest::Client;
use tokio_retry::Retry;
use url::Url;

// Simple wrapper
// maybe extend in the future for our purposes
pub struct BedrockClient(pub CommonHttpClient);

impl BedrockClient {
    pub fn new(auth: Option<BasicAuthCredentials>) -> Result<Self> {
        let client = Client::builder()
                //Add more fiedls if needed
                .timeout(std::time::Duration::from_secs(60))
                .build()?;

        Ok(BedrockClient(CommonHttpClient::new_with_client(
            client, auth,
        )))
    }

    pub async fn get_lib_stream(&self, url: Url) -> Result<impl Stream<Item = BlockInfo>, Error> {
        self.0.get_lib_stream(url).await
    }

    pub async fn get_block_by_id(
        &self,
        url: &Url,
        header_id: HeaderId,
        start_delay_millis: u64,
        max_retries: usize,
    ) -> Result<Option<Block<SignedMantleTx>>, Error> {
        let strategy = tokio_retry::strategy::FibonacciBackoff::from_millis(start_delay_millis)
            .take(max_retries);

        Retry::spawn(strategy, || {
            self.0
                .get_block_by_id(url.clone(), header_id)
                .inspect_err(|err| warn!("Block fetching failed with err: {err:#?}"))
        })
        .await
    }
}

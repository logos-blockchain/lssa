use anyhow::Result;
use futures::{Stream, TryFutureExt};
use log::warn;
pub use logos_blockchain_chain_broadcast_service::BlockInfo;
pub use logos_blockchain_common_http_client::{BasicAuthCredentials, CommonHttpClient, Error};
pub use logos_blockchain_core::{block::Block, header::HeaderId, mantle::SignedMantleTx};
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use tokio_retry::Retry;

/// Fibonacci backoff retry strategy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackoffConfig {
    pub start_delay_millis: u64,
    pub max_retries: usize,
}

// Simple wrapper
// maybe extend in the future for our purposes
// `Clone` is cheap because `CommonHttpClient` is internally reference counted (`Arc`).
#[derive(Clone)]
pub struct BedrockClient {
    http_client: CommonHttpClient,
    node_url: Url,
}

impl BedrockClient {
    pub fn new(auth: Option<BasicAuthCredentials>, node_url: Url) -> Result<Self> {
        let client = Client::builder()
                //Add more fields if needed
                .timeout(std::time::Duration::from_secs(60))
                .build()?;

        let http_client = CommonHttpClient::new_with_client(client, auth);
        Ok(Self {
            http_client,
            node_url,
        })
    }

    pub async fn post_transaction(&self, tx: SignedMantleTx) -> Result<(), Error> {
        self.http_client
            .post_transaction(self.node_url.clone(), tx)
            .await
    }

    pub async fn get_lib_stream(&self) -> Result<impl Stream<Item = BlockInfo>, Error> {
        self.http_client.get_lib_stream(self.node_url.clone()).await
    }

    pub async fn get_block_by_id(
        &self,
        header_id: HeaderId,
        backoff: &BackoffConfig,
    ) -> Result<Option<Block<SignedMantleTx>>, Error> {
        let strategy =
            tokio_retry::strategy::FibonacciBackoff::from_millis(backoff.start_delay_millis)
                .take(backoff.max_retries);

        Retry::spawn(strategy, || {
            self.http_client
                .get_block_by_id(self.node_url.clone(), header_id)
                .inspect_err(|err| warn!("Block fetching failed with err: {err:#?}"))
        })
        .await
    }
}

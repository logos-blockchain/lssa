use anyhow::Result;
pub use logos_blockchain_common_http_client::{BasicAuthCredentials, CommonHttpClient, Error};
use logos_blockchain_core::mantle::SignedMantleTx;
use reqwest::{Client, Url};

// Simple wrapper
// maybe extend in the future for our purposes
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
}

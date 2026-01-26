use anyhow::Result;
use common_http_client::CommonHttpClient;
pub use common_http_client::{BasicAuthCredentials, Error};
use reqwest::Client;

// Simple wrapper
// maybe extend in the future for our purposes
pub struct BedrockClient(pub CommonHttpClient);

impl BedrockClient {
    pub fn new(auth: Option<BasicAuthCredentials>) -> Result<Self> {
        let client = Client::builder()
                //Add more fields if needed
                .timeout(std::time::Duration::from_secs(60))
                .build()?;

        Ok(BedrockClient(CommonHttpClient::new_with_client(
            client, auth,
        )))
    }
}

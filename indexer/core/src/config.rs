use std::{fs::File, io::BufReader, path::Path};

use anyhow::{Context as _, Result};
pub use bedrock_client::BackoffConfig;
use common::config::BasicAuth;
pub use logos_blockchain_core::mantle::ops::channel::ChannelId;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BedrockClientConfig {
    /// For individual RPC requests we use Fibonacci backoff retry strategy.
    pub backoff: BackoffConfig,
    pub addr: Url,
    pub auth: Option<BasicAuth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexerConfig {
    pub resubscribe_interval_millis: u64,
    pub bedrock_client_config: BedrockClientConfig,
    pub channel_id: ChannelId,
}

impl IndexerConfig {
    pub fn from_path(config_path: &Path) -> Result<IndexerConfig> {
        let file = File::open(config_path)
            .with_context(|| format!("Failed to open indexer config at {config_path:?}"))?;
        let reader = BufReader::new(file);

        serde_json::from_reader(reader)
            .with_context(|| format!("Failed to parse indexer config at {config_path:?}"))
    }
}

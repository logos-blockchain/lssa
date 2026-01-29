use std::{fs::File, io::BufReader, path::Path};

use anyhow::{Context, Result};
use bedrock_client::BackoffConfig;
use common::config::BasicAuth;
use logos_blockchain_core::mantle::ops::channel::ChannelId;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BedrockClientConfig {
    pub addr: Url,
    pub auth: Option<BasicAuth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexerConfig {
    pub resubscribe_interval_millis: u64,
    /// For individual RPC requests we use Fibonacci backoff retry strategy.
    pub backoff: BackoffConfig,
    pub bedrock_client_config: BedrockClientConfig,
    pub channel_id: ChannelId,
}

impl IndexerConfig {
    pub fn from_path(config_home: &Path) -> Result<IndexerConfig> {
        let file = File::open(config_home)
            .with_context(|| format!("Failed to open indexer config at {config_home:?}"))?;
        let reader = BufReader::new(file);

        serde_json::from_reader(reader)
            .with_context(|| format!("Failed to parse indexer config at {config_home:?}"))
    }
}

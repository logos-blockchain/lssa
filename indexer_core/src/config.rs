use std::{
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use bedrock_client::BackoffConfig;
use common::{
    block::{AccountInitialData, CommitmentsInitialData},
    sequencer_client::BasicAuth,
};
use logos_blockchain_core::mantle::ops::channel::ChannelId;
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
/// ToDo: Expand if necessary
pub struct ClientConfig {
    pub addr: Url,
    pub auth: Option<BasicAuth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Note: For individual RPC requests we use Fibonacci backoff retry strategy
pub struct IndexerConfig {
    /// Home dir of sequencer storage
    pub home: PathBuf,
    /// List of initial accounts data
    pub initial_accounts: Vec<AccountInitialData>,
    /// List of initial commitments
    pub initial_commitments: Vec<CommitmentsInitialData>,
    pub resubscribe_interval_millis: u64,
    pub backoff: BackoffConfig,
    pub bedrock_client_config: ClientConfig,
    pub sequencer_client_config: ClientConfig,
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

use nomos_core::mantle::ops::channel::ChannelId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
/// ToDo: Expand if necessary
pub struct ClientConfig {
    pub addr: String,
    pub auth: Option<(String, Option<String>)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Note: For individual RPC requests we use Fibonacci backoff retry strategy
pub struct IndexerConfig {
    pub resubscribe_interval_millis: u64,
    pub start_delay_millis: u64,
    pub max_retries: usize,
    pub bedrock_client_config: ClientConfig,
    pub sequencer_client_config: ClientConfig,
    pub channel_id: ChannelId,
}

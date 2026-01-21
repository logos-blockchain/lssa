use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Note: For individual RPC requests we use Fibonacci backoff retry strategy
pub struct IndexerConfig {
    pub resubscribe_interval_millis: u64,
    pub start_delay_millis: u64,
    pub max_retries: usize,
}

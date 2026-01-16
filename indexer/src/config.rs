use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexerConfig {
    pub resubscribe_interval: u64,
    pub start_delay: u64,
    pub limit_retry: usize,
}

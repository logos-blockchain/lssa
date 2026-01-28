use std::sync::Arc;

use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct IndexerState {
    // Only one field for now, for testing.
    pub latest_seen_block: Arc<RwLock<u64>>,
}

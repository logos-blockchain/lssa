use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct IndexerState {
    pub latest_seen_block: Arc<RwLock<u64>>,
    pub finality_map: Arc<RwLock<HashMap<u64, u64>>>,
}

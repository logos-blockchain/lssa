use nomos_core::mantle::ops::channel::ChannelId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexerConfig {
    pub resubscribe_interval: u64,
    pub channel_id: ChannelId,
}

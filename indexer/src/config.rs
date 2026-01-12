use nomos_core::mantle::ops::channel::ChannelId;

#[derive(Debug)]
pub struct IndexerConfig {
    pub resubscribe_interval: u64,
    pub channel_id: ChannelId,
}

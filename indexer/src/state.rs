#[derive(Debug)]
pub struct IndexerState {
    // Only one field for now, for testing.
    pub latest_seen_block: u64,
}

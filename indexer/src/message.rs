#[derive(Debug, Clone)]
pub enum IndexerToSequencerMessage {
    FinalizedBlockObserved {
        l1_block_id: u64,
        l2_block_height: u64,
    },
    BlockEnteredChain {
        l2_block_height: u64,
    },
    ChainRestructurization {
        new_l1_last_block_height: u64,
    },
}

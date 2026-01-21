#[derive(Debug, Clone)]
pub enum Message {
    BlockObserved {
        l1_block_id: u64,
        l2_block_height: u64,
    },
}

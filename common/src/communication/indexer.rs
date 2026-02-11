use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    L2BlockFinalized { l2_block_height: u64 },
}

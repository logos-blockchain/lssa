use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequencerConfig {
    ///Home dir of sequencer storage
    pub home: PathBuf,
    ///Genesis id
    pub genesis_id: u64,
    ///If `True`, then adds random sequence of bytes to genesis block
    pub is_genesis_random: bool,
    ///Maximum number of transactions in block
    pub max_num_tx_in_block: usize,
}

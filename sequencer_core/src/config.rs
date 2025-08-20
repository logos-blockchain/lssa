use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
///Helperstruct for account serialization
pub struct AccountInitialData {
    ///Hex encoded `AccountAddress`
    pub addr: String,
    pub balance: u128,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SequencerConfig {
    ///Home dir of sequencer storage
    pub home: PathBuf,
    ///Override rust log (env var logging level)
    pub override_rust_log: Option<String>,
    ///Genesis id
    pub genesis_id: u64,
    ///If `True`, then adds random sequence of bytes to genesis block
    pub is_genesis_random: bool,
    ///Maximum number of transactions in block
    pub max_num_tx_in_block: usize,
    ///Interval in which blocks produced
    pub block_create_timeout_millis: u64,
    ///Port to listen
    pub port: u16,
    ///List of initial accounts data
    pub initial_accounts: Vec<AccountInitialData>,
}

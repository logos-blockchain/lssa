use std::{
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
/// Helperstruct for account serialization
pub struct AccountInitialData {
    /// Hex encoded account id
    pub account_id: String,
    pub balance: u128,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
/// Helperstruct to initialize commitments
pub struct CommitmentsInitialData {
    pub npk: nssa_core::NullifierPublicKey,
    pub account: nssa_core::account::Account,
}

// TODO: Provide default values
#[derive(Clone, Serialize, Deserialize)]
pub struct SequencerConfig {
    /// Home dir of sequencer storage
    pub home: PathBuf,
    /// Override rust log (env var logging level)
    pub override_rust_log: Option<String>,
    /// Genesis id
    pub genesis_id: u64,
    /// If `True`, then adds random sequence of bytes to genesis block
    pub is_genesis_random: bool,
    /// Maximum number of transactions in block
    pub max_num_tx_in_block: usize,
    /// Mempool maximum size
    pub mempool_max_size: usize,
    /// Interval in which blocks produced
    pub block_create_timeout_millis: u64,
    /// Port to listen
    pub port: u16,
    /// List of initial accounts data
    pub initial_accounts: Vec<AccountInitialData>,
    /// List of initial commitments
    pub initial_commitments: Vec<CommitmentsInitialData>,
    /// Sequencer own signing key
    pub signing_key: [u8; 32],
}

impl SequencerConfig {
    pub fn from_path(config_home: &Path) -> Result<SequencerConfig> {
        let file = File::open(config_home)?;
        let reader = BufReader::new(file);

        Ok(serde_json::from_reader(reader)?)
    }
}

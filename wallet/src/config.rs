use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitialAccountData {
    pub address: nssa::Address,
    pub account: nssa_core::account::Account,
    pub pub_sign_key: nssa::PrivateKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentAccountData {
    pub address: nssa::Address,
    pub pub_sign_key: nssa::PrivateKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasConfig {
    /// Gas spent per deploying one byte of data
    pub gas_fee_per_byte_deploy: u64,
    /// Gas spent per reading one byte of data in VM
    pub gas_fee_per_input_buffer_runtime: u64,
    /// Gas spent per one byte of contract data in runtime
    pub gas_fee_per_byte_runtime: u64,
    /// Cost of one gas of runtime in public balance
    pub gas_cost_runtime: u64,
    /// Cost of one gas of deployment in public balance
    pub gas_cost_deploy: u64,
    /// Gas limit for deployment
    pub gas_limit_deploy: u64,
    /// Gas limit for runtime
    pub gas_limit_runtime: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletConfig {
    ///Home dir of sequencer storage
    pub home: PathBuf,
    ///Override rust log (env var logging level)
    pub override_rust_log: Option<String>,
    ///Sequencer URL
    pub sequencer_addr: String,
    ///Sequencer polling duration for new blocks in milliseconds
    pub seq_poll_timeout_millis: u64,
    ///Sequencer polling max number of blocks
    pub seq_poll_max_blocks: usize,
    ///Sequencer polling max number error retries
    pub seq_poll_max_retries: u64,
    ///Sequencer polling error retry delay in milliseconds
    pub seq_poll_retry_delay_millis: u64,
    ///Initial accounts for wallet
    pub initial_accounts: Vec<InitialAccountData>,
}

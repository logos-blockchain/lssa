use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use zkvm::gas_calculator::GasCalculator;

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

impl From<GasConfig> for zkvm::gas_calculator::GasCalculator {
    fn from(value: GasConfig) -> Self {
        GasCalculator::new(
            value.gas_fee_per_byte_deploy,
            value.gas_fee_per_input_buffer_runtime,
            value.gas_fee_per_byte_runtime,
            value.gas_cost_runtime,
            value.gas_cost_deploy,
            value.gas_limit_deploy,
            value.gas_limit_runtime,
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    ///Home dir of sequencer storage
    pub home: PathBuf,
    ///Override rust log (env var logging level)
    pub override_rust_log: Option<String>,
    ///Sequencer URL
    pub sequencer_addr: String,
    ///Sequencer polling duration for new blocks in seconds
    pub seq_poll_timeout_secs: u64,
    ///Port to listen
    pub port: u16,
    ///Gas config
    pub gas_config: GasConfig,
    ///Frequency of snapshots
    pub shapshot_frequency_in_blocks: u64,
}

use accounts::account_core::AccountAddress;
use config::NodeConfig;
use storage::NodeChainStore;

pub mod config;
pub mod executions;
pub mod sequencer_client;
pub mod storage;

pub struct NodeCore {
    pub storage: NodeChainStore,
    pub curr_height: u64,
    pub main_acc_addr: AccountAddress,
    pub node_config: NodeConfig,
}

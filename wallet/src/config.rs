use key_protocol::key_management::KeyChain;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitialAccountDataPublic {
    pub address: String,
    pub pub_sign_key: nssa::PrivateKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentAccountDataPublic {
    pub address: nssa::Address,
    pub pub_sign_key: nssa::PrivateKey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitialAccountDataPrivate {
    pub address: String,
    pub account: nssa_core::account::Account,
    pub key_chain: KeyChain,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentAccountDataPrivate {
    pub address: nssa::Address,
    pub account: nssa_core::account::Account,
    pub key_chain: KeyChain,
}

//Big difference in enum variants sizes
//however it is improbable, that we will have that much accounts, that it will substantialy affect memory
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InitialAccountData {
    Public(InitialAccountDataPublic),
    Private(InitialAccountDataPrivate),
}

//Big difference in enum variants sizes
//however it is improbable, that we will have that much accounts, that it will substantialy affect memory
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PersistentAccountData {
    Public(PersistentAccountDataPublic),
    Private(PersistentAccountDataPrivate),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentStorage {
    pub accounts: Vec<PersistentAccountData>,
    pub last_synced_block: u64,
}

impl InitialAccountData {
    pub fn address(&self) -> nssa::Address {
        match &self {
            Self::Public(acc) => acc.address.parse().unwrap(),
            Self::Private(acc) => acc.address.parse().unwrap(),
        }
    }
}

impl PersistentAccountData {
    pub fn address(&self) -> nssa::Address {
        match &self {
            Self::Public(acc) => acc.address,
            Self::Private(acc) => acc.address,
        }
    }
}

impl From<InitialAccountDataPublic> for InitialAccountData {
    fn from(value: InitialAccountDataPublic) -> Self {
        Self::Public(value)
    }
}

impl From<InitialAccountDataPrivate> for InitialAccountData {
    fn from(value: InitialAccountDataPrivate) -> Self {
        Self::Private(value)
    }
}

impl From<PersistentAccountDataPublic> for PersistentAccountData {
    fn from(value: PersistentAccountDataPublic) -> Self {
        Self::Public(value)
    }
}

impl From<PersistentAccountDataPrivate> for PersistentAccountData {
    fn from(value: PersistentAccountDataPrivate) -> Self {
        Self::Private(value)
    }
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

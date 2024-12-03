use crate::config::NodeConfig;

pub mod json;

pub struct SequencerClient {
    pub client: reqwest::Client,
    pub config: NodeConfig,
}

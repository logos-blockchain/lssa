use serde::Deserialize;

use crate::rpc_primitives::errors::RpcError;

#[derive(Debug, Clone, Deserialize)]
pub struct SequencerRpcError {
    pub jsonrpc: String,
    pub error: RpcError,
    pub id: u64,
}

#[derive(thiserror::Error, Debug)]
pub enum SequencerClientError {
    #[error("HTTP error")]
    HTTPError(reqwest::Error),
    #[error("Serde error")]
    SerdeError(serde_json::Error),
    #[error("Internal error")]
    InternalError(SequencerRpcError),
}

impl From<reqwest::Error> for SequencerClientError {
    fn from(value: reqwest::Error) -> Self {
        SequencerClientError::HTTPError(value)
    }
}

impl From<serde_json::Error> for SequencerClientError {
    fn from(value: serde_json::Error) -> Self {
        SequencerClientError::SerdeError(value)
    }
}

impl From<SequencerRpcError> for SequencerClientError {
    fn from(value: SequencerRpcError) -> Self {
        SequencerClientError::InternalError(value)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExecutionFailureKind {
    #[error("Failed to get account data from sequencer")]
    SequencerError,
    #[error("Inputs amounts does not match outputs")]
    AmountMismatchError,
    #[error("Accounts key not found")]
    KeyNotFoundError,
    #[error("Sequencer client error: {0:?}")]
    SequencerClientError(#[from] SequencerClientError),
    #[error("Can not pay for operation")]
    InsufficientFundsError,
}

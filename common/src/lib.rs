use merkle_tree_public::TreeHashType;
use serde::Deserialize;

pub mod block;
pub mod commitment;
pub mod execution_input;
pub mod merkle_tree_public;
pub mod nullifier;
pub mod rpc_primitives;
pub mod sequencer_client;
pub mod transaction;
pub mod utxo_commitment;

//Module for tests utility functions
pub mod test_utils;

use rpc_primitives::errors::RpcError;

///Account id on blockchain
pub type AccountId = TreeHashType;

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
    #[error("Failed to write into builder err: {0:?}")]
    WriteError(anyhow::Error),
    #[error("Failed to interact with a db err: {0:?}")]
    DBError(anyhow::Error),
    #[error("Failed to build builder err: {0:?}")]
    BuilderError(anyhow::Error),
    #[error("Failed prove execution err: {0:?}")]
    ProveError(anyhow::Error),
    #[error("Failed to decode data from VM: {0:?}")]
    DecodeError(String),
    #[error("Inputs amounts does not match outputs")]
    AmountMismatchError,
    #[error("Sequencer client error: {0:?}")]
    SequencerClientError(#[from] SequencerClientError),
    #[error("Insufficient gas for operation")]
    InsufficientGasError,
    #[error("Can not pay for operation")]
    InsufficientFundsError,
}

impl ExecutionFailureKind {
    pub fn write_error(err: anyhow::Error) -> Self {
        Self::WriteError(err)
    }

    pub fn builder_error(err: anyhow::Error) -> Self {
        Self::BuilderError(err)
    }

    pub fn prove_error(err: anyhow::Error) -> Self {
        Self::ProveError(err)
    }

    pub fn db_error(err: anyhow::Error) -> Self {
        Self::DBError(err)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TransactionSignatureError {
    #[error("invalid signature for transaction body")]
    InvalidSignature,
}

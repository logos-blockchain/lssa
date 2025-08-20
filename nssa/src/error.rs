use std::io;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum NssaError {
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Risc0 error: {0}")]
    ProgramExecutionFailed(String),

    #[error("Program violated execution rules")]
    InvalidProgramBehavior,

    #[error("Serialization error: {0}")]
    InstructionSerializationError(String),

    #[error("Invalid private key")]
    InvalidPrivateKey,

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Invalid Public Key")]
    InvalidPublicKey,
}

use thiserror::Error;

#[derive(Error, Debug)]
pub enum NssaError {
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Operation failed")]
    OperationFailed,

    #[error("Risc0 error: {0}")]
    ProgramExecutionFailed(String),

    #[error("Program violated execution rules")]
    InvalidProgramBehavior,
}

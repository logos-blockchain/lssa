use serde::{Deserialize, Serialize};

use crate::CommitmentHashType;

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct Commitment {
    pub commitment_hash: CommitmentHashType,
}

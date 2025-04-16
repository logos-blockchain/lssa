use serde::{Deserialize, Serialize};

use crate::merkle_tree_public::CommitmentHashType;

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct Commitment {
    pub commitment_hash: CommitmentHashType,
}

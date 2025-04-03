use crate::merkle_tree_public::CommitmentHashType;
use monotree::database::MemoryDB;
use monotree::hasher::Blake3;
use monotree::Monotree;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct Commitment {
    pub commitment_hash: CommitmentHashType,
}

pub struct CommitmentsSparseMerkleTree {
    pub curr_root: Option<CommitmentHashType>,
    pub tree: Monotree<MemoryDB, Blake3>,
    pub hasher: Blake3,
}

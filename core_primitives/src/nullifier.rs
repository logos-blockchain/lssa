use crate::merkle_tree_public::TreeHashType;
use monotree::database::MemoryDB;
use monotree::hasher::Blake3;
use monotree::Monotree;
use serde::{Deserialize, Serialize};

//ToDo: Update Nullifier model, when it is clear
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
///General nullifier object
pub struct UTXONullifier {
    pub utxo_hash: TreeHashType,
}

pub struct NullifierSparseMerkleTree {
    pub curr_root: Option<TreeHashType>,
    pub tree: Monotree<MemoryDB, Blake3>,
    pub hasher: Blake3,
}

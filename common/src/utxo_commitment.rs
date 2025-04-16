use serde::{Deserialize, Serialize};

use crate::merkle_tree_public::TreeHashType;

//ToDo: Update UTXO Commitment model, when it is clear
#[derive(Debug, Serialize, Deserialize, Clone)]
///General commitment object
pub struct UTXOCommitment {
    pub hash: TreeHashType,
}

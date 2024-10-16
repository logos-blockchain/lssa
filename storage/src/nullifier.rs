use serde::{Deserialize, Serialize};

use crate::merkle_tree_public::TreeHashType;

//ToDo: Update Nullifier model, when it is clear
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
///General bullifier object
pub struct UTXONullifier {
    pub utxo_hash: TreeHashType,
}

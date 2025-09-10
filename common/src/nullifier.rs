use serde::{Deserialize, Serialize};

use crate::TreeHashType;

//ToDo: Update Nullifier model, when it is clear
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq, Hash)]
///General nullifier object
pub struct UTXONullifier {
    pub utxo_hash: TreeHashType,
}

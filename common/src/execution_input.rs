use serde::{Deserialize, Serialize};

use crate::merkle_tree_public::TreeHashType;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicNativeTokenSend {
    pub from: TreeHashType,
    pub to: TreeHashType,
    pub balance_to_move: u64,
}

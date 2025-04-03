use serde::{Deserialize, Serialize};
use storage::{merkle_tree_public::TreeHashType, nullifier::UTXONullifier, AccountId};

///Raw asset data
pub type Asset = Vec<u8>;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
///Container for raw utxo payload
pub struct UTXO {
    pub hash: TreeHashType,
    pub owner: AccountId,
    pub nullifier: Option<UTXONullifier>,
    pub asset: Asset,
    // TODO: change to u256
    pub amount: u128,
    pub privacy_flag: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UTXOPayload {
    pub owner: AccountId,
    pub asset: Asset,
    // TODO: change to u256
    pub amount: u128,
    pub privacy_flag: bool,
}

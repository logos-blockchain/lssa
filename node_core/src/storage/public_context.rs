use std::collections::BTreeMap;

use accounts::account_core::{AccountAddress, AccountPublicMask};
use storage::{merkle_tree_public::TreeHashType, AccountId};

///Strucutre, representing context, given to a smart contract on a call
pub struct PublicSCContext {
    pub caller_address: AccountAddress,
    pub caller_balance: u64,
    pub account_masks: BTreeMap<AccountAddress, AccountPublicMask>,
    pub nullifier_store_root: TreeHashType,
    pub comitment_store_root: TreeHashType,
    pub pub_tx_store_root: TreeHashType,
}

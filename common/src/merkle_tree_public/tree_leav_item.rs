use crate::{transaction::TransactionBody, utxo_commitment::UTXOCommitment};

use super::TreeHashType;

pub trait TreeLeavItem {
    fn hash(&self) -> TreeHashType;
}

impl TreeLeavItem for TransactionBody {
    fn hash(&self) -> TreeHashType {
        self.hash()
    }
}

impl TreeLeavItem for UTXOCommitment {
    fn hash(&self) -> TreeHashType {
        self.hash
    }
}

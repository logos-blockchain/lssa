use crate::{transaction::Transaction, utxo_commitment::UTXOCommitment};

use super::TreeHashType;

pub trait TreeLeavItem {
    fn hash(&self) -> TreeHashType;
}

impl TreeLeavItem for Transaction {
    fn hash(&self) -> TreeHashType {
        self.body().hash()
    }
}

impl TreeLeavItem for UTXOCommitment {
    fn hash(&self) -> TreeHashType {
        self.hash
    }
}

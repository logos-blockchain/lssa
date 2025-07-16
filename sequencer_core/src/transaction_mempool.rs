use common::{merkle_tree_public::TreeHashType, transaction::AuthenticatedTransaction};
use mempool::mempoolitem::MemPoolItem;
use serde::{Deserialize, Serialize};

pub struct MempoolTransaction {
    pub tx: AuthenticatedTransaction,
}

impl From<AuthenticatedTransaction> for MempoolTransaction {
    fn from(value: AuthenticatedTransaction) -> Self {
        Self { tx: value }
    }
}

impl MemPoolItem for MempoolTransaction {
    type Identifier = TreeHashType;

    fn identifier(&self) -> Self::Identifier {
        *self.tx.hash()
    }
}

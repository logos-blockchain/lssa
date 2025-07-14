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

// impl Serialize for TransactionMempool {
//     fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//     where
//         S: serde::Serializer,
//     {
//         self.tx.serialize(serializer)
//     }
// }
//
// impl<'de> Deserialize<'de> for TransactionMempool {
//     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//     where
//         D: serde::Deserializer<'de>,
//     {
//         match TransactionBody::deserialize(deserializer) {
//             Ok(tx) => Ok(TransactionMempool { tx }),
//             Err(err) => Err(err),
//         }
//     }
// }

impl MemPoolItem for MempoolTransaction {
    type Identifier = TreeHashType;

    fn identifier(&self) -> Self::Identifier {
        *self.tx.hash()
    }
}

use common::{merkle_tree_public::TreeHashType, transaction::TransactionBody};
use mempool::mempoolitem::MemPoolItem;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct TransactionMempool {
    pub tx: TransactionBody,
}

impl From<TransactionBody> for TransactionMempool {
    fn from(value: TransactionBody) -> Self {
        Self { tx: value }
    }
}

impl Serialize for TransactionMempool {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.tx.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TransactionMempool {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        match TransactionBody::deserialize(deserializer) {
            Ok(tx) => Ok(TransactionMempool { tx }),
            Err(err) => Err(err),
        }
    }
}

impl MemPoolItem for TransactionMempool {
    type Identifier = TreeHashType;

    fn identifier(&self) -> Self::Identifier {
        self.tx.hash()
    }
}

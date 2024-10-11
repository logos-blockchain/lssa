use mempool::mempoolitem::MemPoolItem;
use serde::{Deserialize, Serialize};
use storage::transaction::{Transaction, TxHash};

#[derive(Debug)]
pub struct TransactionMempool {
    pub tx: Transaction,
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
        match Transaction::deserialize(deserializer) {
            Ok(tx) => Ok(TransactionMempool { tx }),
            Err(err) => Err(err),
        }
    }
}

impl MemPoolItem for TransactionMempool {
    type Identifier = TxHash;

    fn identifier(&self) -> Self::Identifier {
        self.tx.hash
    }
}

use borsh::{BorshDeserialize, BorshSerialize};
use log::info;
use serde::{Deserialize, Serialize};

use sha2::{Digest, digest::FixedOutput};

pub type HashType = [u8; 32];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NSSATransaction {
    Public(nssa::PublicTransaction),
    PrivacyPreserving(nssa::PrivacyPreservingTransaction),
    ProgramDeployment(nssa::ProgramDeploymentTransaction),
}

impl From<nssa::PublicTransaction> for NSSATransaction {
    fn from(value: nssa::PublicTransaction) -> Self {
        Self::Public(value)
    }
}

impl From<nssa::PrivacyPreservingTransaction> for NSSATransaction {
    fn from(value: nssa::PrivacyPreservingTransaction) -> Self {
        Self::PrivacyPreserving(value)
    }
}

impl From<nssa::ProgramDeploymentTransaction> for NSSATransaction {
    fn from(value: nssa::ProgramDeploymentTransaction) -> Self {
        Self::ProgramDeployment(value)
    }
}

#[derive(
    Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize,
)]
pub enum TxKind {
    Public,
    PrivacyPreserving,
    ProgramDeployment,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
///General transaction object
pub struct EncodedTransaction {
    pub tx_kind: TxKind,
    ///Encoded blobs of data
    pub encoded_transaction_data: Vec<u8>,
}

impl From<NSSATransaction> for EncodedTransaction {
    fn from(value: NSSATransaction) -> Self {
        match value {
            NSSATransaction::Public(tx) => Self {
                tx_kind: TxKind::Public,
                encoded_transaction_data: tx.to_bytes(),
            },
            NSSATransaction::PrivacyPreserving(tx) => Self {
                tx_kind: TxKind::PrivacyPreserving,
                encoded_transaction_data: tx.to_bytes(),
            },
            NSSATransaction::ProgramDeployment(tx) => Self {
                tx_kind: TxKind::ProgramDeployment,
                encoded_transaction_data: tx.to_bytes(),
            },
        }
    }
}

impl TryFrom<&EncodedTransaction> for NSSATransaction {
    type Error = nssa::error::NssaError;

    fn try_from(value: &EncodedTransaction) -> Result<Self, Self::Error> {
        match value.tx_kind {
            TxKind::Public => nssa::PublicTransaction::from_bytes(&value.encoded_transaction_data)
                .map(|tx| tx.into()),
            TxKind::PrivacyPreserving => {
                nssa::PrivacyPreservingTransaction::from_bytes(&value.encoded_transaction_data)
                    .map(|tx| tx.into())
            }
            TxKind::ProgramDeployment => {
                nssa::ProgramDeploymentTransaction::from_bytes(&value.encoded_transaction_data)
                    .map(|tx| tx.into())
            }
        }
    }
}

impl EncodedTransaction {
    /// Computes and returns the SHA-256 hash of the JSON-serialized representation of `self`.
    pub fn hash(&self) -> HashType {
        let bytes_to_hash = borsh::to_vec(&self).unwrap();
        let mut hasher = sha2::Sha256::new();
        hasher.update(&bytes_to_hash);
        HashType::from(hasher.finalize_fixed())
    }

    pub fn log(&self) {
        info!("Transaction hash is {:?}", hex::encode(self.hash()));
        info!("Transaction tx_kind is {:?}", self.tx_kind);
    }
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, digest::FixedOutput};

    use crate::{
        HashType,
        transaction::{EncodedTransaction, TxKind},
    };

    fn test_transaction_body() -> EncodedTransaction {
        EncodedTransaction {
            tx_kind: TxKind::Public,
            encoded_transaction_data: vec![1, 2, 3, 4],
        }
    }

    #[test]
    fn test_transaction_hash_is_sha256_of_json_bytes() {
        let body = test_transaction_body();
        let expected_hash = {
            let data = borsh::to_vec(&body).unwrap();
            let mut hasher = sha2::Sha256::new();
            hasher.update(&data);
            HashType::from(hasher.finalize_fixed())
        };

        let hash = body.hash();

        assert_eq!(expected_hash, hash);
    }

    #[test]
    fn test_to_bytes_from_bytes() {
        let body = test_transaction_body();

        let body_bytes = borsh::to_vec(&body).unwrap();
        let body_new = borsh::from_slice::<EncodedTransaction>(&body_bytes).unwrap();

        assert_eq!(body, body_new);
    }
}

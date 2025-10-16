use borsh::{BorshDeserialize, BorshSerialize};
use k256::ecdsa::{Signature, SigningKey, VerifyingKey};
use log::info;
use serde::{Deserialize, Serialize};

use sha2::{Digest, digest::FixedOutput};

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

use crate::TreeHashType;

pub type CipherText = Vec<u8>;
pub type Tag = u8;

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

#[derive(Debug, Serialize, Deserialize)]
pub struct MintMoneyPublicTx {
    pub acc: [u8; 32],
    pub amount: u128,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SendMoneyShieldedTx {
    pub acc_sender: [u8; 32],
    pub amount: u128,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SendMoneyDeshieldedTx {
    pub receiver_data: Vec<(u128, [u8; 32])>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OwnedUTXO {
    pub hash: [u8; 32],
    pub owner: [u8; 32],
    pub amount: u128,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OwnedUTXOForPublication {
    pub hash: String,
    pub owner: String,
    pub amount: u128,
}

impl From<OwnedUTXO> for OwnedUTXOForPublication {
    fn from(value: OwnedUTXO) -> Self {
        Self {
            hash: hex::encode(value.hash),
            owner: hex::encode(value.owner),
            amount: value.amount,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UTXOPublication {
    pub utxos: Vec<OwnedUTXO>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ActionData {
    MintMoneyPublicTx(MintMoneyPublicTx),
    SendMoneyShieldedTx(SendMoneyShieldedTx),
    SendMoneyDeshieldedTx(SendMoneyDeshieldedTx),
    UTXOPublication(UTXOPublication),
}

impl ActionData {
    pub fn into_hexed_print(self) -> String {
        match self {
            ActionData::MintMoneyPublicTx(action) => {
                format!(
                    "Account {:?} minted {:?} balance",
                    hex::encode(action.acc),
                    action.amount
                )
            }
            ActionData::SendMoneyDeshieldedTx(action) => {
                format!(
                    "Receivers receipt {:?}",
                    action
                        .receiver_data
                        .into_iter()
                        .map(|(amount, rec)| (amount, hex::encode(rec)))
                        .collect::<Vec<_>>()
                )
            }
            ActionData::SendMoneyShieldedTx(action) => {
                format!(
                    "Shielded send from {:?} for {:?} balance",
                    hex::encode(action.acc_sender),
                    action.amount
                )
            }
            ActionData::UTXOPublication(action) => {
                let pub_own_utxo: Vec<OwnedUTXOForPublication> = action
                    .utxos
                    .into_iter()
                    .map(|owned_utxo| owned_utxo.into())
                    .collect();
                format!("Published utxos {pub_own_utxo:?}")
            }
        }
    }
}

impl EncodedTransaction {
    /// Computes and returns the SHA-256 hash of the JSON-serialized representation of `self`.
    pub fn hash(&self) -> TreeHashType {
        let bytes_to_hash = borsh::to_vec(&self).unwrap();
        let mut hasher = sha2::Sha256::new();
        hasher.update(&bytes_to_hash);
        TreeHashType::from(hasher.finalize_fixed())
    }

    pub fn log(&self) {
        info!("Transaction hash is {:?}", hex::encode(self.hash()));
        info!("Transaction tx_kind is {:?}", self.tx_kind);
    }
}

pub type TransactionSignature = Signature;
pub type SignaturePublicKey = VerifyingKey;
pub type SignaturePrivateKey = SigningKey;

#[cfg(test)]
mod tests {
    use sha2::{Digest, digest::FixedOutput};

    use crate::{
        TreeHashType,
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
            TreeHashType::from(hasher.finalize_fixed())
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

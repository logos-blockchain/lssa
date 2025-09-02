use std::io::{Cursor, Read};

use k256::ecdsa::{
    signature::{Signer, Verifier},
    Signature, SigningKey, VerifyingKey,
};
use log::info;
use serde::{Deserialize, Serialize};

use sha2::{digest::FixedOutput, Digest};

use crate::{block::u32_from_cursor, merkle_tree_public::TreeHashType};

use elliptic_curve::{
    consts::{B0, B1},
    generic_array::GenericArray,
};
use sha2::digest::typenum::{UInt, UTerm};

use crate::TransactionSignatureError;

pub type CipherText = Vec<u8>;
pub type Nonce = GenericArray<u8, UInt<UInt<UInt<UInt<UTerm, B1>, B1>, B0>, B0>>;
pub type Tag = u8;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum TxKind {
    Public,
    PrivacyPreserving,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
///General transaction object
pub struct TransactionBody {
    pub tx_kind: TxKind,
    ///Encoded blobs of data
    pub encoded_transaction_data: Vec<u8>,
}

impl From<nssa::NSSATransaction> for TransactionBody {
    fn from(value: nssa::NSSATransaction) -> Self {
        match value {
            nssa::NSSATransaction::Public(tx) => {
                Self {
                    tx_kind: TxKind::Public,
                    encoded_transaction_data: tx.to_bytes(),
                }
            },
            nssa::NSSATransaction::PrivacyPreserving(tx) => {
                Self {
                    tx_kind: TxKind::PrivacyPreserving,
                    encoded_transaction_data: tx.to_bytes(),
                }
            }
        }
    }
}

impl TryFrom<TransactionBody> for nssa::NSSATransaction {
    type Error = nssa::error::NssaError;

    fn try_from(value: TransactionBody) -> Result<Self, Self::Error> {
        match value.tx_kind {
            TxKind::Public => {
                nssa::PublicTransaction::from_bytes(&value.encoded_transaction_data).map(|tx| tx.into())
            },
            TxKind::PrivacyPreserving => {
                nssa::PrivacyPreservingTransaction::from_bytes(&value.encoded_transaction_data).map(|tx| tx.into())
            },
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

impl TransactionBody {
    /// Computes and returns the SHA-256 hash of the JSON-serialized representation of `self`.
    pub fn hash(&self) -> TreeHashType {
        let bytes_to_hash = self.to_bytes();
        let mut hasher = sha2::Sha256::new();
        hasher.update(&bytes_to_hash);
        TreeHashType::from(hasher.finalize_fixed())
    }

    fn to_bytes(&self) -> Vec<u8> {
        // TODO: Remove `unwrap` by implementing a `to_bytes` method
        // that deterministically encodes all transaction fields to bytes
        // and guarantees serialization will succeed.
        serde_json::to_vec(&self).unwrap()
    }

    fn from_bytes(bytes: Vec<u8>) -> Self {
        serde_json::from_slice(&bytes).unwrap()
    }

    pub fn log(&self) {
        info!("Transaction hash is {:?}", hex::encode(self.hash()));
        info!("Transaction tx_kind is {:?}", self.tx_kind);
    }
}

type TransactionHash = [u8; 32];
pub type TransactionSignature = Signature;
pub type SignaturePublicKey = VerifyingKey;
pub type SignaturePrivateKey = SigningKey;

/// A container for a transaction body with a signature.
/// Meant to be sent through the network to the sequencer
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Transaction {
    body: TransactionBody,
    pub signature: TransactionSignature,
    pub public_key: VerifyingKey,
}

impl Transaction {
    /// Returns a new transaction signed with the provided `private_key`.
    /// The signature is generated over the hash of the body as computed by `body.hash()`
    pub fn new(body: TransactionBody, private_key: SigningKey) -> Transaction {
        let signature: TransactionSignature = private_key.sign(&body.to_bytes());
        let public_key = VerifyingKey::from(&private_key);
        Self {
            body,
            signature,
            public_key,
        }
    }

    /// Converts the transaction into an `AuthenticatedTransaction` by verifying its signature.
    /// Returns an error if the signature verification fails.
    pub fn into_authenticated(self) -> Result<AuthenticatedTransaction, TransactionSignatureError> {
        let hash = self.body.hash();

        self.public_key
            .verify(&self.body.to_bytes(), &self.signature)
            .map_err(|_| TransactionSignatureError::InvalidSignature)?;

        Ok(AuthenticatedTransaction {
            hash,
            transaction: self,
        })
    }

    /// Returns the body of the transaction
    pub fn body(&self) -> &TransactionBody {
        &self.body
    }
}

impl Transaction {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        let body_bytes = self.body.to_bytes();
        let signature_bytes = self.signature.to_bytes();
        let public_key_bytes = self.public_key.to_sec1_bytes();

        let body_bytes_len = body_bytes.len() as u32;
        let signature_bytes_len = signature_bytes.len() as u32;
        let public_key_bytes_len = public_key_bytes.len() as u32;

        bytes.extend_from_slice(&body_bytes_len.to_le_bytes());
        bytes.extend_from_slice(&signature_bytes_len.to_le_bytes());
        bytes.extend_from_slice(&public_key_bytes_len.to_le_bytes());

        bytes.extend_from_slice(&body_bytes);
        bytes.extend_from_slice(&signature_bytes);
        bytes.extend_from_slice(&public_key_bytes);

        bytes
    }

    // TODO: Improve error handling. Remove unwraps.
    pub fn from_cursor(cursor: &mut Cursor<&[u8]>) -> Self {
        let body_bytes_len = u32_from_cursor(cursor) as usize;
        let signature_bytes_len = u32_from_cursor(cursor) as usize;
        let public_key_bytes_len = u32_from_cursor(cursor) as usize;

        let mut body_bytes = Vec::with_capacity(body_bytes_len);
        let mut signature_bytes = Vec::with_capacity(signature_bytes_len);
        let mut public_key_bytes = Vec::with_capacity(public_key_bytes_len);

        cursor.read_exact(&mut body_bytes).unwrap();
        let body = TransactionBody::from_bytes(body_bytes);

        cursor.read_exact(&mut signature_bytes).unwrap();
        let signature = Signature::from_bytes(signature_bytes.as_slice().try_into().unwrap()).unwrap();

        cursor.read_exact(&mut public_key_bytes).unwrap();
        let public_key = VerifyingKey::from_sec1_bytes(&public_key_bytes).unwrap();

        Self { body, signature, public_key }
    }
}

/// A transaction with a valid signature over the hash of its body.
/// Can only be constructed from an `Transaction`
/// if the signature is valid
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedTransaction {
    hash: TransactionHash,
    transaction: Transaction,
}

impl AuthenticatedTransaction {
    /// Returns the underlying transaction
    pub fn transaction(&self) -> &Transaction {
        &self.transaction
    }

    pub fn into_transaction(self) -> Transaction {
        self.transaction
    }

    /// Returns the precomputed hash over the body of the transaction
    pub fn hash(&self) -> &TransactionHash {
        &self.hash
    }
}

impl AuthenticatedTransaction {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.hash);
        bytes.extend_from_slice(&self.transaction.to_bytes());
        bytes
    }

    // TODO: Improve error handling. Remove unwraps.
    pub fn from_cursor(cursor: &mut Cursor<&[u8]>) -> Self { 
        let mut hash: [u8; 32] = [0; 32];
        cursor.read_exact(&mut hash).unwrap();
        let transaction = Transaction::from_cursor(cursor);
        Self { hash, transaction }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::{ecdsa::signature::Signer, FieldBytes};
    use sha2::{digest::FixedOutput, Digest};

    use crate::{
        merkle_tree_public::TreeHashType,
        transaction::{Transaction, TransactionBody, TxKind},
    };

    fn test_transaction_body() -> TransactionBody {
        TransactionBody {
            tx_kind: TxKind::Public,
            encoded_transaction_data: vec![1,2,3,4],
        }
    }

    fn test_transaction() -> Transaction {
        let body = test_transaction_body();
        let key_bytes = FieldBytes::from_slice(&[37; 32]);
        let private_key: SigningKey = SigningKey::from_bytes(key_bytes).unwrap();
        Transaction::new(body, private_key)
    }

    #[test]
    fn test_transaction_hash_is_sha256_of_json_bytes() {
        let body = test_transaction_body();
        let expected_hash = {
            let data = serde_json::to_vec(&body).unwrap();
            let mut hasher = sha2::Sha256::new();
            hasher.update(&data);
            TreeHashType::from(hasher.finalize_fixed())
        };

        let hash = body.hash();

        assert_eq!(expected_hash, hash);
    }

    #[test]
    fn test_transaction_constructor() {
        let body = test_transaction_body();
        let key_bytes = FieldBytes::from_slice(&[37; 32]);
        let private_key: SigningKey = SigningKey::from_bytes(key_bytes).unwrap();
        let transaction = Transaction::new(body.clone(), private_key.clone());
        assert_eq!(
            transaction.public_key,
            SignaturePublicKey::from(&private_key)
        );
        assert_eq!(transaction.body, body);
    }

    #[test]
    fn test_transaction_body_getter() {
        let body = test_transaction_body();
        let key_bytes = FieldBytes::from_slice(&[37; 32]);
        let private_key: SigningKey = SigningKey::from_bytes(key_bytes).unwrap();
        let transaction = Transaction::new(body.clone(), private_key.clone());
        assert_eq!(transaction.body(), &body);
    }

    #[test]
    fn test_into_authenticated_succeeds_for_valid_signature() {
        let transaction = test_transaction();
        let authenticated_tx = transaction.clone().into_authenticated().unwrap();

        let signature = authenticated_tx.transaction().signature;
        let hash = authenticated_tx.hash();

        assert_eq!(authenticated_tx.transaction(), &transaction);
        assert_eq!(hash, &transaction.body.hash());
        assert!(authenticated_tx
            .transaction()
            .public_key
            .verify(&transaction.body.to_bytes(), &signature)
            .is_ok());
    }

    #[test]
    fn test_into_authenticated_fails_for_invalid_signature() {
        let body = test_transaction_body();
        let key_bytes = FieldBytes::from_slice(&[37; 32]);
        let private_key: SigningKey = SigningKey::from_bytes(key_bytes).unwrap();
        let transaction = {
            let mut this = Transaction::new(body, private_key.clone());
            // Modify the signature to make it invalid
            // We do this by changing it to the signature of something else
            this.signature = private_key.sign(b"deadbeef");
            this
        };

        matches!(
            transaction.into_authenticated(),
            Err(TransactionSignatureError::InvalidSignature)
        );
    }

    #[test]
    fn test_authenticated_transaction_getter() {
        let transaction = test_transaction();
        let authenticated_tx = transaction.clone().into_authenticated().unwrap();
        assert_eq!(authenticated_tx.transaction(), &transaction);
    }

    #[test]
    fn test_authenticated_transaction_hash_getter() {
        let transaction = test_transaction();
        let authenticated_tx = transaction.clone().into_authenticated().unwrap();
        assert_eq!(authenticated_tx.hash(), &transaction.body.hash());
    }

    #[test]
    fn test_authenticated_transaction_into_transaction() {
        let transaction = test_transaction();
        let authenticated_tx = transaction.clone().into_authenticated().unwrap();
        assert_eq!(authenticated_tx.into_transaction(), transaction);
    }
}

use k256::ecdsa::{
    signature::{Signer, Verifier},
    Signature, SigningKey, VerifyingKey,
};
use log::info;
use secp256k1_zkp::{PedersenCommitment, Tweak};
use serde::{Deserialize, Serialize};

use sha2::{digest::FixedOutput, Digest};

use crate::merkle_tree_public::TreeHashType;

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
    Private,
    Shielded,
    Deshielded,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
///General transaction object
pub struct TransactionBody {
    pub tx_kind: TxKind,
    ///Tx input data (public part)
    pub execution_input: Vec<u8>,
    ///Tx output data (public_part)
    pub execution_output: Vec<u8>,
    ///Tx input utxo commitments
    pub utxo_commitments_spent_hashes: Vec<TreeHashType>,
    ///Tx output utxo commitments
    pub utxo_commitments_created_hashes: Vec<TreeHashType>,
    ///Tx output nullifiers
    pub nullifier_created_hashes: Vec<TreeHashType>,
    ///Execution proof (private part)
    pub execution_proof_private: String,
    ///Encoded blobs of data
    pub encoded_data: Vec<(CipherText, Vec<u8>, Tag)>,
    ///Transaction senders ephemeral pub key
    pub ephemeral_pub_key: Vec<u8>,
    ///Public (Pedersen) commitment
    pub commitment: Vec<PedersenCommitment>,
    ///tweak
    pub tweak: Tweak,
    ///secret_r
    pub secret_r: [u8; 32],
    ///Hex-encoded address of a smart contract account called
    pub sc_addr: String,
    ///Recorded changes in state of smart contract
    ///
    /// First value represents vector of changes, second is new length of a state
    pub state_changes: (serde_json::Value, usize),
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

    pub fn log(&self) {
        info!("Transaction hash is {:?}", hex::encode(self.hash()));
        info!("Transaction tx_kind is {:?}", self.tx_kind);
        info!("Transaction execution_input is {:?}", {
            if let Ok(action) = serde_json::from_slice::<ActionData>(&self.execution_input) {
                action.into_hexed_print()
            } else {
                "".to_string()
            }
        });
        info!("Transaction execution_output is {:?}", {
            if let Ok(action) = serde_json::from_slice::<ActionData>(&self.execution_output) {
                action.into_hexed_print()
            } else {
                "".to_string()
            }
        });
        info!(
            "Transaction utxo_commitments_spent_hashes is {:?}",
            self.utxo_commitments_spent_hashes
                .iter()
                .map(|val| hex::encode(*val))
                .collect::<Vec<_>>()
        );
        info!(
            "Transaction utxo_commitments_created_hashes is {:?}",
            self.utxo_commitments_created_hashes
                .iter()
                .map(|val| hex::encode(*val))
                .collect::<Vec<_>>()
        );
        info!(
            "Transaction nullifier_created_hashes is {:?}",
            self.nullifier_created_hashes
                .iter()
                .map(|val| hex::encode(*val))
                .collect::<Vec<_>>()
        );
        info!(
            "Transaction encoded_data is {:?}",
            self.encoded_data
                .iter()
                .map(|val| (hex::encode(val.0.clone()), hex::encode(val.1.clone())))
                .collect::<Vec<_>>()
        );
        info!(
            "Transaction ephemeral_pub_key is {:?}",
            hex::encode(self.ephemeral_pub_key.clone())
        );
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

/// A transaction with a valid signature over the hash of its body.
/// Can only be constructed from an `Transaction`
/// if the signature is valid
#[derive(Debug, Clone)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use k256::{ecdsa::signature::Signer, FieldBytes};
    use secp256k1_zkp::{constants::SECRET_KEY_SIZE, Tweak};
    use sha2::{digest::FixedOutput, Digest};

    use crate::{
        merkle_tree_public::TreeHashType,
        transaction::{Transaction, TransactionBody, TxKind},
    };

    fn test_transaction_body() -> TransactionBody {
        TransactionBody {
            tx_kind: TxKind::Public,
            execution_input: vec![1, 2, 3, 4],
            execution_output: vec![5, 6, 7, 8],
            utxo_commitments_spent_hashes: vec![[9; 32], [10; 32], [11; 32], [12; 32]],
            utxo_commitments_created_hashes: vec![[13; 32]],
            nullifier_created_hashes: vec![[0; 32], [1; 32], [2; 32], [3; 32]],
            execution_proof_private: "loremipsum".to_string(),
            encoded_data: vec![(vec![255, 255, 255], vec![254, 254, 254], 1)],
            ephemeral_pub_key: vec![5; 32],
            commitment: vec![],
            tweak: Tweak::from_slice(&[7; SECRET_KEY_SIZE]).unwrap(),
            secret_r: [8; 32],
            sc_addr: "someAddress".to_string(),
            state_changes: (serde_json::Value::Null, 10),
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

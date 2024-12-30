use log::info;
use serde::{Deserialize, Serialize};
use sha2::{digest::FixedOutput, Digest};

use crate::merkle_tree_public::TreeHashType;

use elliptic_curve::{
    consts::{B0, B1},
    generic_array::GenericArray,
};
use sha2::digest::typenum::{UInt, UTerm};

pub type CipherText = Vec<u8>;
pub type Nonce = GenericArray<u8, UInt<UInt<UInt<UInt<UTerm, B1>, B1>, B0>, B0>>;

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum TxKind {
    Public,
    Private,
    Shielded,
    Deshielded,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
///General transaction object
pub struct Transaction {
    pub hash: TreeHashType,
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
    pub encoded_data: Vec<(CipherText, Vec<u8>)>,
    ///Transaction senders ephemeral pub key
    pub ephemeral_pub_key: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
///General transaction object
pub struct TransactionPayload {
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
    pub encoded_data: Vec<(CipherText, Vec<u8>)>,
    ///Transaction senders ephemeral pub key
    pub ephemeral_pub_key: Vec<u8>,
}

impl From<TransactionPayload> for Transaction {
    fn from(value: TransactionPayload) -> Self {
        let raw_data = serde_json::to_vec(&value).unwrap();

        let mut hasher = sha2::Sha256::new();

        hasher.update(&raw_data);

        let hash = <TreeHashType>::from(hasher.finalize_fixed());

        Self {
            hash,
            tx_kind: value.tx_kind,
            execution_input: value.execution_input,
            execution_output: value.execution_output,
            utxo_commitments_spent_hashes: value.utxo_commitments_spent_hashes,
            utxo_commitments_created_hashes: value.utxo_commitments_created_hashes,
            nullifier_created_hashes: value.nullifier_created_hashes,
            execution_proof_private: value.execution_proof_private,
            encoded_data: value.encoded_data,
            ephemeral_pub_key: value.ephemeral_pub_key,
        }
    }
}

impl Transaction {
    pub fn log(&self) {
        info!("Transaction hash is {:?}", hex::encode(self.hash));
        info!("Transaction tx_kind is {:?}", self.tx_kind);
        info!(
            "Transaction execution_input is {:?}",
            hex::encode(self.execution_input.clone())
        );
        info!(
            "Transaction execution_output is {:?}",
            hex::encode(self.execution_output.clone())
        );
        info!(
            "Transaction utxo_commitments_spent_hashes is {:?}",
            self.utxo_commitments_spent_hashes
                .iter()
                .map(|val| hex::encode(val.clone()))
        );
        info!(
            "Transaction utxo_commitments_created_hashes is {:?}",
            self.utxo_commitments_created_hashes
                .iter()
                .map(|val| hex::encode(val.clone()))
        );
        info!(
            "Transaction nullifier_created_hashes is {:?}",
            self.nullifier_created_hashes
                .iter()
                .map(|val| hex::encode(val.clone()))
        );
        info!(
            "Transaction execution_proof_private is {:?}",
            self.execution_proof_private
        );
        info!(
            "Transaction encoded_data is {:?}",
            self.encoded_data
                .iter()
                .map(|val| (hex::encode(val.0.clone()), hex::encode(val.1.clone())))
        );
        info!(
            "Transaction ephemeral_pub_key is {:?}",
            hex::encode(self.ephemeral_pub_key.clone())
        );
    }
}

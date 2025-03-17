use serde::{Deserialize, Serialize};

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

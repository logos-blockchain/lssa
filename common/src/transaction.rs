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

pub type CipherText = Vec<u8>;
pub type Nonce = GenericArray<u8, UInt<UInt<UInt<UInt<UTerm, B1>, B1>, B0>, B0>>;
pub type Tag = u8;

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
    pub encoded_data: Vec<(CipherText, Vec<u8>, Tag)>,
    ///Transaction senders ephemeral pub key
    pub ephemeral_pub_key: Vec<u8>,
    ///Public (Pedersen) commitment
    pub commitment: Vec<PedersenCommitment>,
    ///tweak
    pub tweak: Tweak,
    ///secret_r
    pub secret_r: [u8; 32],
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
    pub encoded_data: Vec<(CipherText, Vec<u8>, Tag)>,
    ///Transaction senders ephemeral pub key
    pub ephemeral_pub_key: Vec<u8>,
    ///Public (Pedersen) commitment
    pub commitment: Vec<PedersenCommitment>,
    ///tweak
    pub tweak: Tweak,
    ///secret_r
    pub secret_r: [u8; 32],
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
            commitment: value.commitment,
            tweak: value.tweak,
            secret_r: value.secret_r,
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
                format!("Published utxos {:?}", pub_own_utxo)
            }
        }
    }
}

impl Transaction {
    pub fn log(&self) {
        info!("Transaction hash is {:?}", hex::encode(self.hash));
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
                .map(|val| hex::encode(val.clone()))
                .collect::<Vec<_>>()
        );
        info!(
            "Transaction utxo_commitments_created_hashes is {:?}",
            self.utxo_commitments_created_hashes
                .iter()
                .map(|val| hex::encode(val.clone()))
                .collect::<Vec<_>>()
        );
        info!(
            "Transaction nullifier_created_hashes is {:?}",
            self.nullifier_created_hashes
                .iter()
                .map(|val| hex::encode(val.clone()))
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

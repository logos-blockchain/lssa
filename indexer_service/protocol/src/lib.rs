//! This crate defines the protocol types used by the indexer service.
//!
//! Currently it mostly mimics types from `nssa_core`, but it's important to have a separate crate
//! to define a stable interface for the indexer service RPCs which evolves in its own way.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[cfg(feature = "convert")]
mod convert;

pub type Nonce = u128;

pub type ProgramId = [u32; 8];

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct AccountId {
    #[serde(with = "base64::arr")]
    #[schemars(with = "String", description = "base64-encoded account ID")]
    pub value: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Account {
    pub program_owner: ProgramId,
    pub balance: u128,
    pub data: Data,
    pub nonce: Nonce,
}

pub type BlockId = u64;
pub type TimeStamp = u64;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Block {
    pub header: BlockHeader,
    pub body: BlockBody,
    pub bedrock_status: BedrockStatus,
    pub bedrock_parent_id: MantleMsgId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct BlockHeader {
    pub block_id: BlockId,
    pub prev_block_hash: Hash,
    pub hash: Hash,
    pub timestamp: TimeStamp,
    pub signature: Signature,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Signature(
    #[serde(with = "base64::arr")]
    #[schemars(with = "String", description = "base64-encoded signature")]
    pub [u8; 64],
);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct BlockBody {
    pub transactions: Vec<Transaction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum Transaction {
    Public(PublicTransaction),
    PrivacyPreserving(PrivacyPreservingTransaction),
    ProgramDeployment(ProgramDeploymentTransaction),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct PublicTransaction {
    pub message: PublicMessage,
    pub witness_set: WitnessSet,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct PrivacyPreservingTransaction {
    pub message: PrivacyPreservingMessage,
    pub witness_set: WitnessSet,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct PublicMessage {
    pub program_id: ProgramId,
    pub account_ids: Vec<AccountId>,
    pub nonces: Vec<Nonce>,
    pub instruction_data: InstructionData,
}

pub type InstructionData = Vec<u32>;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct PrivacyPreservingMessage {
    pub public_account_ids: Vec<AccountId>,
    pub nonces: Vec<Nonce>,
    pub public_post_states: Vec<Account>,
    pub encrypted_private_post_states: Vec<EncryptedAccountData>,
    pub new_commitments: Vec<Commitment>,
    pub new_nullifiers: Vec<(Nullifier, CommitmentSetDigest)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct WitnessSet {
    pub signatures_and_public_keys: Vec<(Signature, PublicKey)>,
    pub proof: Proof,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Proof(
    #[serde(with = "base64")]
    #[schemars(with = "String", description = "base64-encoded proof")]
    pub Vec<u8>,
);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct EncryptedAccountData {
    pub ciphertext: Ciphertext,
    pub epk: EphemeralPublicKey,
    pub view_tag: ViewTag,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct ProgramDeploymentTransaction {
    pub message: ProgramDeploymentMessage,
}

pub type ViewTag = u8;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Ciphertext(
    #[serde(with = "base64")]
    #[schemars(with = "String", description = "base64-encoded ciphertext")]
    pub Vec<u8>,
);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct PublicKey(
    #[serde(with = "base64::arr")]
    #[schemars(with = "String", description = "base64-encoded public key")]
    pub [u8; 32],
);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct EphemeralPublicKey(
    #[serde(with = "base64")]
    #[schemars(with = "String", description = "base64-encoded ephemeral public key")]
    pub Vec<u8>,
);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Commitment(
    #[serde(with = "base64::arr")]
    #[schemars(with = "String", description = "base64-encoded commitment")]
    pub [u8; 32],
);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Nullifier(
    #[serde(with = "base64::arr")]
    #[schemars(with = "String", description = "base64-encoded nullifier")]
    pub [u8; 32],
);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct CommitmentSetDigest(
    #[serde(with = "base64::arr")]
    #[schemars(with = "String", description = "base64-encoded commitment set digest")]
    pub [u8; 32],
);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct ProgramDeploymentMessage {
    #[serde(with = "base64")]
    #[schemars(with = "String", description = "base64-encoded program bytecode")]
    pub bytecode: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Data(
    #[serde(with = "base64")]
    #[schemars(with = "String", description = "base64-encoded account data")]
    pub Vec<u8>,
);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Hash(
    #[serde(with = "base64::arr")]
    #[schemars(with = "String", description = "base64-encoded hash")]
    pub [u8; 32],
);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct MantleMsgId(
    #[serde(with = "base64::arr")]
    #[schemars(with = "String", description = "base64-encoded Bedrock message id")]
    pub [u8; 32],
);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum BedrockStatus {
    Pending,
    Safe,
    Finalized,
}

mod base64 {
    use base64::prelude::{BASE64_STANDARD, Engine as _};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub mod arr {
        use super::*;

        pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
            super::serialize(v, s)
        }

        pub fn deserialize<'de, const N: usize, D: Deserializer<'de>>(
            d: D,
        ) -> Result<[u8; N], D::Error> {
            let vec = super::deserialize(d)?;
            vec.try_into().map_err(|_| {
                serde::de::Error::custom(format!("Invalid length, expected {N} bytes"))
            })
        }
    }

    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        let base64 = BASE64_STANDARD.encode(v);
        String::serialize(&base64, s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let base64 = String::deserialize(d)?;
        BASE64_STANDARD
            .decode(base64.as_bytes())
            .map_err(serde::de::Error::custom)
    }
}

//! This crate defines the protocol types used by the indexer service.
//!
//! Currently it mostly mimics types from `nssa_core`, but it's important to have a separate crate
//! to define a stable interface for the indexer service RPCs which evolves in its own way.

use std::{collections::HashSet, fmt::Display, str::FromStr};

use anyhow::anyhow;
use base58::{FromBase58 as _, ToBase58 as _};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use sha2::{Digest, Sha256};

#[cfg(feature = "convert")]
mod convert;

pub type Nonce = u128;

#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Hash, SerializeDisplay, DeserializeFromStr, JsonSchema,
)]
pub struct ProgramId(pub [u32; 8]);

impl Display for ProgramId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let bytes: Vec<u8> = self.0.iter().flat_map(|n| n.to_be_bytes()).collect();
        write!(f, "{}", bytes.to_base58())
    }
}

impl FromStr for ProgramId {
    type Err = hex::FromHexError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = s
            .from_base58()
            .map_err(|_| hex::FromHexError::InvalidStringLength)?;
        if bytes.len() != 32 {
            return Err(hex::FromHexError::InvalidStringLength);
        }
        let mut arr = [0u32; 8];
        for (i, chunk) in bytes.chunks_exact(4).enumerate() {
            arr[i] = u32::from_be_bytes(chunk.try_into().unwrap());
        }
        Ok(ProgramId(arr))
    }
}

#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Hash, SerializeDisplay, DeserializeFromStr, JsonSchema,
)]
pub struct AccountId {
    pub value: [u8; 32],
}

impl From<&PublicKey> for AccountId {
    fn from(key: &PublicKey) -> Self {
        const PUBLIC_ACCOUNT_ID_PREFIX: &[u8; 32] =
            b"/LEE/v0.3/AccountId/Public/\x00\x00\x00\x00\x00";

        let mut hasher = Sha256::new();
        hasher.update(PUBLIC_ACCOUNT_ID_PREFIX);
        hasher.update(key.0);
        Self{ value: hasher.finalize().into()}
    }
} 

impl Display for AccountId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value.to_base58())
    }
}

impl FromStr for AccountId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = s
            .from_base58()
            .map_err(|err| anyhow!("invalid base58: {err:?}"))?;
        if bytes.len() != 32 {
            return Err(anyhow!(
                "invalid length: expected 32 bytes, got {}",
                bytes.len()
            ));
        }
        let mut value = [0u8; 32];
        value.copy_from_slice(&bytes);
        Ok(AccountId { value })
    }
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
    pub prev_block_hash: HashType,
    pub hash: HashType,
    pub timestamp: TimeStamp,
    pub signature: Signature,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, SerializeDisplay, DeserializeFromStr, JsonSchema)]
pub struct Signature(
    #[schemars(with = "String", description = "hex-encoded signature")] pub [u8; 64],
);

impl Display for Signature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl FromStr for Signature {
    type Err = hex::FromHexError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut bytes = [0u8; 64];
        hex::decode_to_slice(s, &mut bytes)?;
        Ok(Signature(bytes))
    }
}

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

impl Transaction {
    /// Get the hash of the transaction
    pub fn hash(&self) -> &self::HashType {
        match self {
            Transaction::Public(tx) => &tx.hash,
            Transaction::PrivacyPreserving(tx) => &tx.hash,
            Transaction::ProgramDeployment(tx) => &tx.hash,
        }
    }

    /// Get affected public account ids
    pub fn affected_public_account_ids(&self) -> Vec<AccountId> {
        match self {
            Transaction::Public(tx) => tx.affected_public_account_ids(),
            Transaction::PrivacyPreserving(tx) => tx.affected_public_account_ids(),
            Transaction::ProgramDeployment(tx) => tx.affected_public_account_ids(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct PublicTransaction {
    pub hash: HashType,
    pub message: PublicMessage,
    pub witness_set: WitnessSet,
}

impl PublicTransaction {


    pub(crate) fn signer_account_ids(&self) -> Vec<AccountId> {
        self.witness_set
            .signatures_and_public_keys()
            .iter()
            .map(|(_, public_key)| AccountId::from(public_key))
            .collect()
    }

    pub fn affected_public_account_ids(&self) -> Vec<AccountId> {
        let mut acc_set = self
            .signer_account_ids()
            .into_iter()
            .collect::<HashSet<_>>();
        acc_set.extend(&self.message.account_ids);

        acc_set.into_iter().collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct PrivacyPreservingTransaction {
    pub hash: HashType,
    pub message: PrivacyPreservingMessage,
    pub witness_set: WitnessSet,
}

impl PrivacyPreservingTransaction {
    pub(crate) fn signer_account_ids(&self) -> Vec<AccountId> {
        self.witness_set
            .signatures_and_public_keys()
            .iter()
            .map(|(_, public_key)| AccountId::from(public_key))
            .collect()
    }

    pub fn affected_public_account_ids(&self) -> Vec<AccountId> {
        let mut acc_set = self
            .signer_account_ids()
            .into_iter()
            .collect::<HashSet<_>>();
        acc_set.extend(&self.message.public_account_ids);

        acc_set.into_iter().collect()
    }
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

impl WitnessSet {
    pub fn signatures_and_public_keys(&self) -> &[(Signature, PublicKey)] {
        &self.signatures_and_public_keys
    }
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
    pub hash: HashType,
    pub message: ProgramDeploymentMessage,
}

impl ProgramDeploymentTransaction {
    pub fn affected_public_account_ids(&self) -> Vec<AccountId> {
        vec![]
    }
}

pub type ViewTag = u8;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Ciphertext(
    #[serde(with = "base64")]
    #[schemars(with = "String", description = "base64-encoded ciphertext")]
    pub Vec<u8>,
);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Commitment(
    #[serde(with = "base64::arr")]
    #[schemars(with = "String", description = "base64-encoded commitment")]
    pub [u8; 32],
);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct Nullifier(
    #[serde(with = "base64::arr")]
    #[schemars(with = "String", description = "base64-encoded nullifier")]
    pub [u8; 32],
);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
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

#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Hash, SerializeDisplay, DeserializeFromStr, JsonSchema,
)]
pub struct HashType(pub [u8; 32]);

impl Display for HashType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl FromStr for HashType {
    type Err = hex::FromHexError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut bytes = [0u8; 32];
        hex::decode_to_slice(s, &mut bytes)?;
        Ok(HashType(bytes))
    }
}

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

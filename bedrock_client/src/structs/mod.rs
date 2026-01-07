use ed25519_dalek::Signature;
use serde::{Deserialize, Serialize};

use crate::structs::{header_id::{ContentId, HeaderId}, info::Slot};

pub mod header_id;
pub mod info;
pub mod signature;
pub mod tx;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockInfo {
    pub height: u64,
    pub header_id: HeaderId,
}

pub const BEDROCK_VERSION: u8 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Copy, Serialize, Deserialize)]
#[repr(u8)]
pub enum Version {
    Bedrock = BEDROCK_VERSION,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Header {
    version: Version,
    parent_block: HeaderId,
    slot: Slot,
    block_root: ContentId,
    // Not sure, if need this.
    // proof_of_leadership: Groth16LeaderProof,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Block<Tx> {
    header: Header,
    signature: Signature,
    transactions: Vec<Tx>,
}
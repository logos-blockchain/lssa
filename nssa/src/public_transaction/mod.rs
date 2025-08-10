use nssa_core::{account::Nonce, program::ProgramId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, digest::FixedOutput};

use crate::{
    address::Address,
    signature::{PrivateKey, PublicKey, Signature},
};

mod message;
mod witness_set;

pub use message::Message;
pub use witness_set::WitnessSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublicTransaction {
    message: Message,
    witness_set: WitnessSet,
}

impl PublicTransaction {
    pub fn message(&self) -> &Message {
        &self.message
    }

    pub fn witness_set(&self) -> &WitnessSet {
        &self.witness_set
    }

    pub(crate) fn signer_addresses(&self) -> Vec<Address> {
        self.witness_set
            .signatures_and_public_keys
            .iter()
            .map(|(_, public_key)| Address::from_public_key(public_key))
            .collect()
    }

    pub fn new(message: Message, witness_set: WitnessSet) -> Self {
        Self {
            message,
            witness_set,
        }
    }

    pub fn hash(&self) -> [u8; 32] {
        let bytes = serde_cbor::to_vec(&self).unwrap();
        let mut hasher = sha2::Sha256::new();
        hasher.update(&bytes);
        hasher.finalize_fixed().into()
    }
}

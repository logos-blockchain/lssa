use nssa_core::{account::Nonce, program::ProgramId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, digest::FixedOutput};

use crate::{
    address::Address,
    signature::{PrivateKey, PublicKey, Signature},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Message {
    pub(crate) program_id: ProgramId,
    pub(crate) addresses: Vec<Address>,
    pub(crate) nonces: Vec<Nonce>,
    // TODO: change to Vec<u8> for general programs
    pub(crate) instruction_data: u128,
}

impl Message {
    pub fn new(
        program_id: ProgramId,
        addresses: Vec<Address>,
        nonces: Vec<Nonce>,
        instruction_data: u128,
    ) -> Self {
        Self {
            program_id,
            addresses,
            nonces,
            instruction_data,
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        //TODO: implement
        vec![0, 0]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WitnessSet {
    pub signatures_and_public_keys: Vec<(Signature, PublicKey)>,
}

impl WitnessSet {
    pub fn for_message(message: &Message, private_keys: &[&PrivateKey]) -> Self {
        let message_bytes = message.to_bytes();
        let signatures_and_public_keys = private_keys
            .iter()
            .map(|&key| (Signature::new(key, &message_bytes), PublicKey::new(key)))
            .collect();
        Self {
            signatures_and_public_keys,
        }
    }
}

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

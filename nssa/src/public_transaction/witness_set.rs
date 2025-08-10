use serde::{Deserialize, Serialize};

use crate::{PrivateKey, PublicKey, Signature, public_transaction::Message};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WitnessSet {
    pub(crate) signatures_and_public_keys: Vec<(Signature, PublicKey)>,
}

fn serialize_message_to_bytes(message: &Message) -> Vec<u8> {
    //TODO: implement
    vec![0, 0]
}

impl WitnessSet {
    pub fn for_message(message: &Message, private_keys: &[&PrivateKey]) -> Self {
        let message_bytes = serialize_message_to_bytes(&message);
        let signatures_and_public_keys = private_keys
            .iter()
            .map(|&key| (Signature::new(key, &message_bytes), PublicKey::new(key)))
            .collect();
        Self {
            signatures_and_public_keys,
        }
    }

    pub fn iter_signatures(&self) -> impl Iterator<Item = &(Signature, PublicKey)> {
        self.signatures_and_public_keys.iter()
    }
}

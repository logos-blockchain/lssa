use std::io::{Cursor, Read};

use serde::{Deserialize, Serialize};

use crate::{PrivateKey, PublicKey, Signature, error::NssaError, public_transaction::Message};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitnessSet {
    pub(crate) signatures_and_public_keys: Vec<(Signature, PublicKey)>,
}

impl WitnessSet {
    pub fn for_message(message: &Message, private_keys: &[&PrivateKey]) -> Self {
        let message_bytes = message.message_to_bytes();
        let signatures_and_public_keys = private_keys
            .iter()
            .map(|&key| (Signature::new(key, &message_bytes), PublicKey::new(key)))
            .collect();
        Self {
            signatures_and_public_keys,
        }
    }

    pub fn is_valid_for(&self, message: &Message) -> bool {
        let message_bytes = message.message_to_bytes();
        for (signature, public_key) in self.iter_signatures() {
            if !signature.is_valid_for(&message_bytes, &public_key) {
                return false;
            }
        }
        true
    }

    pub fn iter_signatures(&self) -> impl Iterator<Item = &(Signature, PublicKey)> {
        self.signatures_and_public_keys.iter()
    }

}

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

    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        let size = self.signatures_and_public_keys.len() as u32;
        bytes.extend_from_slice(&size.to_le_bytes());
        for (signature, public_key) in &self.signatures_and_public_keys {
            bytes.extend_from_slice(&signature.value);
            bytes.extend_from_slice(&public_key.0);
        }
        bytes
    }

    // TODO: remove unwraps and return Result
    pub(crate) fn from_cursor(cursor: &mut Cursor<&[u8]>) -> Self {
        let num_signatures: u32 = {
            let mut buf = [0u8; 4];
            cursor.read_exact(&mut buf).unwrap();
            u32::from_le_bytes(buf)
        };
        let mut signatures_and_public_keys = Vec::with_capacity(num_signatures as usize);
        for i in 0..num_signatures {
            let signature = Signature::from_cursor(cursor);
            let public_key = PublicKey::from_cursor(cursor);
            signatures_and_public_keys.push((signature, public_key))
        }
        Self {
            signatures_and_public_keys,
        }
    }
}

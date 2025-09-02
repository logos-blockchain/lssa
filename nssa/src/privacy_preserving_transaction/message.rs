use nssa_core::{
    Commitment, CommitmentSetDigest, Nullifier, PrivacyPreservingCircuitOutput,
    account::{Account, Nonce},
    encryption::{Ciphertext, EphemeralPublicKey},
};

use crate::{Address, error::NssaError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptedAccountData {
    pub(crate) ciphertext: Ciphertext,
    pub(crate) epk: EphemeralPublicKey,
    pub(crate) view_tag: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub(crate) public_addresses: Vec<Address>,
    pub(crate) nonces: Vec<Nonce>,
    pub(crate) public_post_states: Vec<Account>,
    pub(crate) encrypted_private_post_states: Vec<EncryptedAccountData>,
    pub(crate) new_commitments: Vec<Commitment>,
    pub(crate) new_nullifiers: Vec<(Nullifier, CommitmentSetDigest)>,
}

impl Message {
    pub fn try_from_circuit_output(
        public_addresses: Vec<Address>,
        nonces: Vec<Nonce>,
        ephemeral_public_keys: Vec<EphemeralPublicKey>,
        output: PrivacyPreservingCircuitOutput,
    ) -> Result<Self, NssaError> {
        if ephemeral_public_keys.len() != output.ciphertexts.len() {
            return Err(NssaError::InvalidInput(
                "Ephemeral public keys and ciphertexts length mismatch".into(),
            ));
        }

        let encrypted_private_post_states = output
            .ciphertexts
            .into_iter()
            .zip(ephemeral_public_keys)
            .map(|(ciphertext, epk)| EncryptedAccountData {
                ciphertext,
                epk,
                view_tag: 0, // TODO: implement
            })
            .collect();
        Ok(Self {
            public_addresses,
            nonces,
            public_post_states: output.public_post_states,
            encrypted_private_post_states,
            new_commitments: output.new_commitments,
            new_nullifiers: output.new_nullifiers,
        })
    }
}

#[cfg(test)]
pub mod tests {
    use std::io::Cursor;

    use nssa_core::{Commitment, Nullifier, NullifierPublicKey, account::Account};

    use crate::{Address, privacy_preserving_transaction::message::Message};

    pub fn message_for_tests() -> Message {
        let account1 = Account::default();
        let account2 = Account::default();

        let nsk1 = [11; 32];
        let nsk2 = [12; 32];

        let npk1 = NullifierPublicKey::from(&nsk1);
        let npk2 = NullifierPublicKey::from(&nsk2);

        let public_addresses = vec![Address::new([1; 32])];

        let nonces = vec![1, 2, 3];

        let public_post_states = vec![Account::default()];

        let encrypted_private_post_states = Vec::new();

        let new_commitments = vec![Commitment::new(&npk2, &account2)];

        let old_commitment = Commitment::new(&npk1, &account1);
        let new_nullifiers = vec![(Nullifier::new(&old_commitment, &nsk1), [0; 32])];

        Message {
            public_addresses: public_addresses.clone(),
            nonces: nonces.clone(),
            public_post_states: public_post_states.clone(),
            encrypted_private_post_states: encrypted_private_post_states.clone(),
            new_commitments: new_commitments.clone(),
            new_nullifiers: new_nullifiers.clone(),
        }
    }

    #[test]
    fn test_message_serialization_roundtrip() {
        let message = message_for_tests();

        let bytes = message.to_bytes();
        let mut cursor = Cursor::new(bytes.as_ref());
        let message_from_cursor = Message::from_cursor(&mut cursor).unwrap();

        assert_eq!(message, message_from_cursor);
    }
}

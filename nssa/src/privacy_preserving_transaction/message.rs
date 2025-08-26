use std::io::Cursor;

use k256::{
    AffinePoint, EncodedPoint, FieldBytes, ProjectivePoint, PublicKey, Scalar,
    elliptic_curve::{
        PrimeField,
        sec1::{FromEncodedPoint, ToEncodedPoint},
    },
};
use nssa_core::{
    Ciphertext, CommitmentSetDigest, PrivacyPreservingCircuitOutput, SharedSecretKey,
    account::{Account, Commitment, Nonce, Nullifier, NullifierPublicKey},
};
use serde::{Deserialize, Serialize};

use crate::{Address, error::NssaError};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Secp256k1Point(pub(crate) Vec<u8>);
impl Secp256k1Point {
    pub fn from_scalar(value: [u8; 32]) -> Secp256k1Point {
        let x_bytes: FieldBytes = value.into();
        let x = Scalar::from_repr(x_bytes).unwrap();

        let p = ProjectivePoint::GENERATOR * x;
        let q = AffinePoint::from(p);
        let enc = q.to_encoded_point(true);

        Self(enc.as_bytes().to_vec())
    }
}

pub type EphemeralSecretKey = [u8; 32];
pub type EphemeralPublicKey = Secp256k1Point;
pub type IncomingViewingPublicKey = Secp256k1Point;
impl From<&EphemeralSecretKey> for EphemeralPublicKey {
    fn from(value: &EphemeralSecretKey) -> Self {
        Secp256k1Point::from_scalar(*value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptedAccountData {
    pub(crate) ciphertext: Ciphertext,
    pub(crate) epk: EphemeralPublicKey,
    pub(crate) view_tag: u8,
}

impl EncryptedAccountData {
    pub fn decrypt(
        self,
        isk: &[u8; 32],
        epk: &EphemeralPublicKey,
        npk: &NullifierPublicKey,
        output_index: u32,
    ) -> Option<Account> {
        let shared_secret = Self::compute_shared_secret(isk, &epk);
        self.ciphertext.decrypt(&shared_secret, npk, output_index)
    }

    pub fn compute_shared_secret(scalar: &[u8; 32], point: &Secp256k1Point) -> SharedSecretKey {
        let scalar = Scalar::from_repr((*scalar).into()).unwrap();
        let point: [u8; 33] = point.0.clone().try_into().unwrap();

        let encoded = EncodedPoint::from_bytes(point).unwrap();
        let pubkey_affine = AffinePoint::from_encoded_point(&encoded).unwrap();

        let shared = ProjectivePoint::from(pubkey_affine) * scalar;
        let shared_affine = shared.to_affine();

        let encoded = shared_affine.to_encoded_point(false);
        let x_bytes_slice = encoded.x().unwrap();
        let mut x_bytes = [0u8; 32];
        x_bytes.copy_from_slice(x_bytes_slice);

        x_bytes
    }
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

    use nssa_core::account::{
        Account, Commitment, Nullifier, NullifierPublicKey, NullifierSecretKey,
    };

    use crate::{Address, privacy_preserving_transaction::message::Message};

    pub fn message_for_tests() -> Message {
        let account1 = Account::default();
        let account2 = Account::default();

        let nsk1 = [11; 32];
        let nsk2 = [12; 32];

        let Npk1 = NullifierPublicKey::from(&nsk1);
        let Npk2 = NullifierPublicKey::from(&nsk2);

        let public_addresses = vec![Address::new([1; 32])];

        let nonces = vec![1, 2, 3];

        let public_post_states = vec![Account::default()];

        let encrypted_private_post_states = Vec::new();

        let new_commitments = vec![Commitment::new(&Npk2, &account2)];

        let old_commitment = Commitment::new(&Npk1, &account1);
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

use nssa_core::{
    Commitment, CommitmentSetDigest, Nullifier, NullifierPublicKey, PrivacyPreservingCircuitOutput,
    account::{Account, Nonce},
    encryption::{Ciphertext, EphemeralPublicKey, IncomingViewingPublicKey},
};
use sha2::{Digest, Sha256};

use crate::{Address, error::NssaError};

pub type ViewTag = u8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncryptedAccountData {
    pub ciphertext: Ciphertext,
    pub epk: EphemeralPublicKey,
    pub view_tag: ViewTag,
}

impl EncryptedAccountData {
    fn new(
        ciphertext: Ciphertext,
        npk: NullifierPublicKey,
        ivk: IncomingViewingPublicKey,
        epk: EphemeralPublicKey,
    ) -> Self {
        let view_tag = Self::compute_view_tag(npk, ivk);
        Self {
            ciphertext,
            epk,
            view_tag,
        }
    }

    /// Computes the tag as the first byte of SHA256("/NSSA/v0.2/ViewTag/" || Npk || Ivk)
    pub fn compute_view_tag(npk: NullifierPublicKey, ivk: IncomingViewingPublicKey) -> ViewTag {
        let mut hasher = Sha256::new();
        hasher.update(b"/NSSA/v0.2/ViewTag/");
        hasher.update(npk.to_byte_array());
        hasher.update(ivk.to_bytes());
        let digest: [u8; 32] = hasher.finalize().into();
        digest[0]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub(crate) public_addresses: Vec<Address>,
    pub(crate) nonces: Vec<Nonce>,
    pub(crate) public_post_states: Vec<Account>,
    pub encrypted_private_post_states: Vec<EncryptedAccountData>,
    pub new_commitments: Vec<Commitment>,
    pub(crate) new_nullifiers: Vec<(Nullifier, CommitmentSetDigest)>,
}

impl Message {
    pub fn try_from_circuit_output(
        public_addresses: Vec<Address>,
        nonces: Vec<Nonce>,
        public_keys: Vec<(
            NullifierPublicKey,
            IncomingViewingPublicKey,
            EphemeralPublicKey,
        )>,
        output: PrivacyPreservingCircuitOutput,
    ) -> Result<Self, NssaError> {
        if public_keys.len() != output.ciphertexts.len() {
            return Err(NssaError::InvalidInput(
                "Ephemeral public keys and ciphertexts length mismatch".into(),
            ));
        }

        let encrypted_private_post_states = output
            .ciphertexts
            .into_iter()
            .zip(public_keys)
            .map(|(ciphertext, (npk, ivk, epk))| {
                EncryptedAccountData::new(ciphertext, npk, ivk, epk)
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

    use nssa_core::{
        Commitment, EncryptionScheme, Nullifier, NullifierPublicKey, SharedSecretKey,
        account::Account,
        encryption::{EphemeralPublicKey, IncomingViewingPublicKey},
    };
    use sha2::{Digest, Sha256};

    use crate::{
        Address,
        privacy_preserving_transaction::message::{EncryptedAccountData, Message},
    };

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
        let new_nullifiers = vec![(
            Nullifier::for_account_update(&old_commitment, &nsk1),
            [0; 32],
        )];

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

    #[test]
    fn test_encrypted_account_data_constructor() {
        let npk = NullifierPublicKey::from(&[1; 32]);
        let ivk = IncomingViewingPublicKey::from_scalar([2; 32]);
        let account = Account::default();
        let commitment = Commitment::new(&npk, &account);
        let esk = [3; 32];
        let shared_secret = SharedSecretKey::new(&esk, &ivk);
        let epk = EphemeralPublicKey::from_scalar(esk);
        let ciphertext = EncryptionScheme::encrypt(&account, &shared_secret, &commitment, 2);
        let encrypted_account_data =
            EncryptedAccountData::new(ciphertext.clone(), npk.clone(), ivk.clone(), epk.clone());

        let expected_view_tag = {
            let mut hasher = Sha256::new();
            hasher.update(b"/NSSA/v0.2/ViewTag/");
            hasher.update(npk.to_byte_array());
            hasher.update(ivk.to_bytes());
            let digest: [u8; 32] = hasher.finalize().into();
            digest[0]
        };

        assert_eq!(encrypted_account_data.ciphertext, ciphertext);
        assert_eq!(encrypted_account_data.epk, epk);
        assert_eq!(
            encrypted_account_data.view_tag,
            EncryptedAccountData::compute_view_tag(npk, ivk)
        );
        assert_eq!(encrypted_account_data.view_tag, expected_view_tag);
    }
}

use std::io::{Cursor, Read};

use nssa_core::{
    Commitment, Nullifier,
    account::Account,
    encryption::{Ciphertext, EphemeralPublicKey},
};

use crate::{
    Address, PrivacyPreservingTransaction, PublicKey, Signature,
    error::NssaError,
    privacy_preserving_transaction::{
        circuit::Proof,
        message::{EncryptedAccountData, Message},
        witness_set::WitnessSet,
    },
};

const MESSAGE_ENCODING_PREFIX_LEN: usize = 22;
const MESSAGE_ENCODING_PREFIX: &[u8; MESSAGE_ENCODING_PREFIX_LEN] = b"\x01/NSSA/v0.1/TxMessage/";

impl EncryptedAccountData {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = self.ciphertext.to_bytes();
        bytes.extend_from_slice(&self.epk.to_bytes());
        bytes.push(self.view_tag);
        bytes
    }

    pub fn from_cursor(cursor: &mut Cursor<&[u8]>) -> Result<Self, NssaError> {
        let ciphertext = Ciphertext::from_cursor(cursor)?;
        let epk = EphemeralPublicKey::from_cursor(cursor)?;

        let mut tag_bytes = [0; 1];
        cursor.read_exact(&mut tag_bytes)?;
        let view_tag = tag_bytes[0];

        Ok(Self {
            ciphertext,
            epk,
            view_tag,
        })
    }
}

impl Message {
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = MESSAGE_ENCODING_PREFIX.to_vec();

        // Public addresses
        let public_addresses_len: u32 = self.public_addresses.len() as u32;
        bytes.extend_from_slice(&public_addresses_len.to_le_bytes());
        for address in &self.public_addresses {
            bytes.extend_from_slice(address.value());
        }
        // Nonces
        let nonces_len = self.nonces.len() as u32;
        bytes.extend(&nonces_len.to_le_bytes());
        for nonce in &self.nonces {
            bytes.extend(&nonce.to_le_bytes());
        }
        // Public post states
        let public_post_states_len: u32 = self.public_post_states.len() as u32;
        bytes.extend_from_slice(&public_post_states_len.to_le_bytes());
        for account in &self.public_post_states {
            bytes.extend_from_slice(&account.to_bytes());
        }

        // Encrypted post states
        let encrypted_accounts_post_states_len: u32 =
            self.encrypted_private_post_states.len() as u32;
        bytes.extend_from_slice(&encrypted_accounts_post_states_len.to_le_bytes());
        for encrypted_account in &self.encrypted_private_post_states {
            bytes.extend_from_slice(&encrypted_account.to_bytes());
        }

        // New commitments
        let new_commitments_len: u32 = self.new_commitments.len() as u32;
        bytes.extend_from_slice(&new_commitments_len.to_le_bytes());
        for commitment in &self.new_commitments {
            bytes.extend_from_slice(&commitment.to_byte_array());
        }

        // New nullifiers
        let new_nullifiers_len: u32 = self.new_nullifiers.len() as u32;
        bytes.extend_from_slice(&new_nullifiers_len.to_le_bytes());
        for (nullifier, commitment_set_digest) in &self.new_nullifiers {
            bytes.extend_from_slice(&nullifier.to_byte_array());
            bytes.extend_from_slice(commitment_set_digest);
        }

        bytes
    }

    #[allow(unused)]
    pub(crate) fn from_cursor(cursor: &mut Cursor<&[u8]>) -> Result<Self, NssaError> {
        let prefix = {
            let mut this = [0u8; MESSAGE_ENCODING_PREFIX_LEN];
            cursor.read_exact(&mut this)?;
            this
        };
        if &prefix != MESSAGE_ENCODING_PREFIX {
            return Err(NssaError::TransactionDeserializationError(
                "Invalid privacy preserving message prefix".to_string(),
            ));
        }

        let mut len_bytes = [0u8; 4];

        // Public addresses
        cursor.read_exact(&mut len_bytes)?;
        let public_addresses_len = u32::from_le_bytes(len_bytes) as usize;
        let mut public_addresses = Vec::with_capacity(public_addresses_len);
        for _ in 0..public_addresses_len {
            let mut value = [0u8; 32];
            cursor.read_exact(&mut value)?;
            public_addresses.push(Address::new(value))
        }

        // Nonces
        cursor.read_exact(&mut len_bytes)?;
        let nonces_len = u32::from_le_bytes(len_bytes) as usize;
        let mut nonces = Vec::with_capacity(nonces_len);
        for _ in 0..nonces_len {
            let mut buf = [0u8; 16];
            cursor.read_exact(&mut buf)?;
            nonces.push(u128::from_le_bytes(buf))
        }

        // Public post states
        cursor.read_exact(&mut len_bytes)?;
        let public_post_states_len = u32::from_le_bytes(len_bytes) as usize;
        let mut public_post_states = Vec::with_capacity(public_post_states_len);
        for _ in 0..public_post_states_len {
            public_post_states.push(Account::from_cursor(cursor)?);
        }

        // Encrypted private post states
        cursor.read_exact(&mut len_bytes)?;
        let encrypted_len = u32::from_le_bytes(len_bytes) as usize;
        let mut encrypted_private_post_states = Vec::with_capacity(encrypted_len);
        for _ in 0..encrypted_len {
            encrypted_private_post_states.push(EncryptedAccountData::from_cursor(cursor)?);
        }

        // New commitments
        cursor.read_exact(&mut len_bytes)?;
        let new_commitments_len = u32::from_le_bytes(len_bytes) as usize;
        let mut new_commitments = Vec::with_capacity(new_commitments_len);
        for _ in 0..new_commitments_len {
            new_commitments.push(Commitment::from_cursor(cursor)?);
        }

        // New nullifiers
        cursor.read_exact(&mut len_bytes)?;
        let new_nullifiers_len = u32::from_le_bytes(len_bytes) as usize;
        let mut new_nullifiers = Vec::with_capacity(new_nullifiers_len);
        for _ in 0..new_nullifiers_len {
            let nullifier = Nullifier::from_cursor(cursor)?;
            let mut commitment_set_digest = [0; 32];
            cursor.read_exact(&mut commitment_set_digest)?;
            new_nullifiers.push((nullifier, commitment_set_digest));
        }

        Ok(Self {
            public_addresses,
            nonces,
            public_post_states,
            encrypted_private_post_states,
            new_commitments,
            new_nullifiers,
        })
    }
}

impl WitnessSet {
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        let size = self.signatures_and_public_keys().len() as u32;
        bytes.extend_from_slice(&size.to_le_bytes());
        for (signature, public_key) in self.signatures_and_public_keys() {
            bytes.extend_from_slice(signature.to_bytes());
            bytes.extend_from_slice(public_key.to_bytes());
        }
        bytes.extend_from_slice(&self.proof.to_bytes());
        bytes
    }

    pub(crate) fn from_cursor(cursor: &mut Cursor<&[u8]>) -> Result<Self, NssaError> {
        let num_signatures: u32 = {
            let mut buf = [0u8; 4];
            cursor.read_exact(&mut buf)?;
            u32::from_le_bytes(buf)
        };
        let mut signatures_and_public_keys = Vec::with_capacity(num_signatures as usize);
        for _i in 0..num_signatures {
            let signature = Signature::from_cursor(cursor)?;
            let public_key = PublicKey::from_cursor(cursor)?;
            signatures_and_public_keys.push((signature, public_key))
        }
        let proof = Proof::from_cursor(cursor)?;
        Ok(Self {
            signatures_and_public_keys,
            proof,
        })
    }
}

impl PrivacyPreservingTransaction {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = self.message().to_bytes();
        bytes.extend_from_slice(&self.witness_set().to_bytes());
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, NssaError> {
        let mut cursor = Cursor::new(bytes);
        Self::from_cursor(&mut cursor)
    }

    pub fn from_cursor(cursor: &mut Cursor<&[u8]>) -> Result<Self, NssaError> {
        let message = Message::from_cursor(cursor)?;
        let witness_set = WitnessSet::from_cursor(cursor)?;
        Ok(PrivacyPreservingTransaction::new(message, witness_set))
    }
}

impl Proof {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        let proof_len = self.0.len() as u32;
        bytes.extend_from_slice(&proof_len.to_le_bytes());
        bytes.extend_from_slice(&self.0);
        bytes
    }

    pub fn from_cursor(cursor: &mut Cursor<&[u8]>) -> Result<Self, NssaError> {
        let proof_len = u32_from_cursor(cursor) as usize;
        let mut proof = Vec::with_capacity(proof_len);

        for _ in 0..proof_len {
            let mut one_byte_buf = [0u8];

            cursor.read_exact(&mut one_byte_buf)?;

            proof.push(one_byte_buf[0]);
        }
        Ok(Self(proof))
    }
}

// TODO: Improve error handling. Remove unwraps.
pub fn u32_from_cursor(cursor: &mut Cursor<&[u8]>) -> u32 {
    let mut word_buf = [0u8; 4];
    cursor.read_exact(&mut word_buf).unwrap();
    u32::from_le_bytes(word_buf)
}

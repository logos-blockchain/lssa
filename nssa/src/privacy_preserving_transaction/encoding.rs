use std::io::{Cursor, Read};

use nssa_core::{
    Commitment, Nullifier,
    account::Account,
    encryption::{Ciphertext, EphemeralPublicKey},
};

use crate::{
    Address, error::NssaError, privacy_preserving_transaction::message::EncryptedAccountData,
};

use super::message::Message;

const MESSAGE_ENCODING_PREFIX_LEN: usize = 22;
const MESSAGE_ENCODING_PREFIX: &[u8; MESSAGE_ENCODING_PREFIX_LEN] = b"\x01/NSSA/v0.1/TxMessage/";

impl EncryptedAccountData {
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
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

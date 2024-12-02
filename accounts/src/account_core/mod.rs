use std::collections::HashMap;

use anyhow::Result;
use k256::AffinePoint;
use serde::Serialize;
use storage::{merkle_tree_public::TreeHashType, nullifier::UTXONullifier};
use utxo::{
    utxo_core::{UTXOPayload, UTXO},
    utxo_tree::UTXOSparseMerkleTree,
};

use crate::key_management::{
    constants_types::{CipherText, Nonce},
    ephemeral_key_holder::EphemeralKeyHolder,
    AddressKeyHolder,
};

pub type PublicKey = AffinePoint;
pub type AccountAddress = TreeHashType;

pub struct Account {
    pub key_holder: AddressKeyHolder,
    pub address: AccountAddress,
    pub balance: u64,
    pub utxo_tree: UTXOSparseMerkleTree,
}

impl Account {
    pub fn new() -> Self {
        let key_holder = AddressKeyHolder::new_os_random();
        let address = key_holder.address;
        let balance = 0;
        let utxo_tree = UTXOSparseMerkleTree::new();

        Self {
            key_holder,
            address,
            balance,
            utxo_tree,
        }
    }

    pub fn produce_ephemeral_key_holder(&self) -> EphemeralKeyHolder {
        self.key_holder.produce_ephemeral_key_holder()
    }

    pub fn encrypt_data(
        ephemeral_key_holder: &EphemeralKeyHolder,
        viewing_public_key_receiver: AffinePoint,
        data: &[u8],
    ) -> (CipherText, Nonce) {
        ephemeral_key_holder.encrypt_data(viewing_public_key_receiver, data)
    }

    pub fn decrypt_data(
        &self,
        ephemeral_public_key_sender: AffinePoint,
        ciphertext: CipherText,
        nonce: Nonce,
    ) -> Vec<u8> {
        self.key_holder
            .decrypt_data(ephemeral_public_key_sender, ciphertext, nonce)
    }

    pub fn mark_spent_utxo(
        &mut self,
        utxo_nullifier_map: HashMap<TreeHashType, UTXONullifier>,
    ) -> Result<()> {
        for (hash, nullifier) in utxo_nullifier_map {
            if let Some(utxo_entry) = self.utxo_tree.store.get_mut(&hash) {
                utxo_entry.consume_utxo(nullifier)?;
            }
        }

        Ok(())
    }

    pub fn add_new_utxo_outputs(&mut self, utxos: Vec<UTXO>) -> Result<()> {
        Ok(self.utxo_tree.insert_items(utxos)?)
    }

    pub fn update_public_balance(&mut self, new_balance: u64) {
        self.balance = new_balance;
    }

    pub fn add_asset<Asset: Serialize>(
        &mut self,
        asset: Asset,
        amount: u128,
        privacy_flag: bool,
    ) -> Result<()> {
        let payload_with_asset = UTXOPayload {
            owner: self.address,
            asset: serde_json::to_vec(&asset)?,
            amount,
            privacy_flag,
        };

        let asset_utxo = UTXO::create_utxo_from_payload(payload_with_asset);

        self.utxo_tree.insert_item(asset_utxo)?;

        Ok(())
    }
}

impl Default for Account {
    fn default() -> Self {
        Self::new()
    }
}

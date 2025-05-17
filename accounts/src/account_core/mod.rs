use std::{collections::HashMap, hash::Hash};

use anyhow::Result;
use common::{merkle_tree_public::TreeHashType, nullifier::UTXONullifier, transaction::Tag};
use k256::AffinePoint;
use log::info;
use serde::Serialize;
use utxo::utxo_core::{UTXOPayload, UTXO};

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
    pub utxos: HashMap<TreeHashType, UTXO>,
}

///A strucure, which represents all the visible(public) information
///
/// known to each node about account `address`
#[derive(Serialize, Clone)]
pub struct AccountPublicMask {
    pub nullifier_public_key: AffinePoint,
    pub viewing_public_key: AffinePoint,
    pub address: AccountAddress,
    pub balance: u64,
}

impl Account {
    pub fn new() -> Self {
        let key_holder = AddressKeyHolder::new_os_random();
        let address = key_holder.address;
        let balance = 0;
        let utxos = HashMap::new();

        Self {
            key_holder,
            address,
            balance,
            utxos,
        }
    }

    pub fn new_with_balance(balance: u64) -> Self {
        let key_holder = AddressKeyHolder::new_os_random();
        let address = key_holder.address;
        let utxos = HashMap::new();

        Self {
            key_holder,
            address,
            balance,
            utxos,
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
    ) -> Result<Vec<u8>, aes_gcm::Error> {
        self.key_holder
            .decrypt_data(ephemeral_public_key_sender, ciphertext, nonce)
    }

    pub fn mark_spent_utxo(
        &mut self,
        utxo_nullifier_map: HashMap<TreeHashType, UTXONullifier>,
    ) -> Result<()> {
        for (hash, nullifier) in utxo_nullifier_map {
            if let Some(utxo_entry) = self.utxos.get_mut(&hash) {
                utxo_entry.consume_utxo(nullifier)?;
            }
        }

        Ok(())
    }

    pub fn add_new_utxo_outputs(&mut self, utxos: Vec<UTXO>) -> Result<()> {
        for utxo in utxos {
            if self.utxos.contains_key(&utxo.hash) {
                return Err(anyhow::anyhow!("UTXO already exists"));
            }
            self.utxos.insert(utxo.hash, utxo);
        }
        return Ok(());
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

        let asset_utxo = UTXO::create_utxo_from_payload(payload_with_asset)?;

        self.utxos.insert(asset_utxo.hash, asset_utxo);

        Ok(())
    }

    pub fn log(&self) {
        info!("Keys generated");
        info!("Account address is {:?}", hex::encode(self.address));
        info!("Account balance is {:?}", self.balance);
    }

    pub fn make_tag(&self) -> Tag {
        self.address[0]
    }

    ///Produce account public mask
    pub fn make_account_public_mask(&self) -> AccountPublicMask {
        AccountPublicMask {
            nullifier_public_key: self.key_holder.nullifer_public_key,
            viewing_public_key: self.key_holder.viewing_public_key,
            address: self.address,
            balance: self.balance,
        }
    }
}

impl Default for Account {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generate_dummy_utxo_nullifier() -> UTXONullifier {
        UTXONullifier::default()
    }

    fn generate_dummy_utxo(address: TreeHashType, amount: u128) -> anyhow::Result<UTXO> {
        let payload = UTXOPayload {
            owner: address,
            asset: vec![],
            amount,
            privacy_flag: false,
        };
        UTXO::create_utxo_from_payload(payload)
    }

    #[test]
    fn test_new_account() {
        let account = Account::new();

        assert_eq!(account.balance, 0);
        assert!(account.key_holder.address != [0u8; 32]); // Check if the address is not empty
    }

    #[test]
    fn test_mark_spent_utxo() {
        let mut account = Account::new();
        let utxo = generate_dummy_utxo(account.address, 100).unwrap();
        account.add_new_utxo_outputs(vec![utxo]).unwrap();

        let mut utxo_nullifier_map = HashMap::new();
        utxo_nullifier_map.insert(account.address, generate_dummy_utxo_nullifier());

        let result = account.mark_spent_utxo(utxo_nullifier_map);

        assert!(result.is_ok());
    }

    #[test]
    fn test_add_new_utxo_outputs() {
        let mut account = Account::new();
        let utxo1 = generate_dummy_utxo(account.address, 100).unwrap();
        let utxo2 = generate_dummy_utxo(account.address, 200).unwrap();

        let result = account.add_new_utxo_outputs(vec![utxo1.clone(), utxo2.clone()]);

        assert!(result.is_ok());
        assert_eq!(account.utxos.len(), 2);
    }

    #[test]
    fn test_update_public_balance() {
        let mut account = Account::new();
        account.update_public_balance(500);

        assert_eq!(account.balance, 500);
    }

    #[test]
    fn test_add_asset() {
        let mut account = Account::new();
        let asset = "dummy_asset";
        let amount = 1000u128;

        let result = account.add_asset(asset, amount, false);

        assert!(result.is_ok());
        assert_eq!(account.utxos.len(), 1);
    }
}

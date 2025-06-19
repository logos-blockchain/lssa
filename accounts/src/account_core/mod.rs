use std::collections::HashMap;

use anyhow::Result;
use common::{merkle_tree_public::TreeHashType, transaction::Tag};
use k256::AffinePoint;
use log::info;
use serde::{Deserialize, Serialize};
use utxo::utxo_core::UTXO;

use crate::key_management::{
    constants_types::{CipherText, Nonce},
    ephemeral_key_holder::EphemeralKeyHolder,
    AddressKeyHolder,
};

pub type PublicKey = AffinePoint;
pub type AccountAddress = TreeHashType;

#[derive(Clone)]
pub struct Account {
    pub key_holder: AddressKeyHolder,
    pub address: AccountAddress,
    pub balance: u64,
    pub utxos: HashMap<TreeHashType, UTXO>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AccountForSerialization {
    pub key_holder: AddressKeyHolder,
    pub address: AccountAddress,
    pub balance: u64,
    pub utxos: HashMap<String, UTXO>,
}

impl From<Account> for AccountForSerialization {
    fn from(value: Account) -> Self {
        AccountForSerialization {
            key_holder: value.key_holder,
            address: value.address,
            balance: value.balance,
            utxos: value
                .utxos
                .into_iter()
                .map(|(key, val)| (hex::encode(key), val))
                .collect(),
        }
    }
}

impl From<AccountForSerialization> for Account {
    fn from(value: AccountForSerialization) -> Self {
        Account {
            key_holder: value.key_holder,
            address: value.address,
            balance: value.balance,
            utxos: value
                .utxos
                .into_iter()
                .map(|(key, val)| (hex::decode(key).unwrap().try_into().unwrap(), val))
                .collect(),
        }
    }
}

impl Serialize for Account {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let account_for_serialization: AccountForSerialization = From::from(self.clone());
        account_for_serialization.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Account {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let account_for_serialization = <AccountForSerialization>::deserialize(deserializer)?;
        Ok(account_for_serialization.into())
    }
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

impl AccountPublicMask {
    pub fn encrypt_data(
        ephemeral_key_holder: &EphemeralKeyHolder,
        viewing_public_key_receiver: AffinePoint,
        data: &[u8],
    ) -> (CipherText, Nonce) {
        ephemeral_key_holder.encrypt_data(viewing_public_key_receiver, data)
    }

    pub fn make_tag(&self) -> Tag {
        self.address[0]
    }

    pub fn produce_ephemeral_key_holder(&self) -> EphemeralKeyHolder {
        EphemeralKeyHolder::new_os_random()
    }
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

    pub fn add_new_utxo_outputs(&mut self, utxos: Vec<UTXO>) -> Result<()> {
        for utxo in utxos {
            if self.utxos.contains_key(&utxo.hash) {
                return Err(anyhow::anyhow!("UTXO already exists"));
            }
            self.utxos.insert(utxo.hash, utxo);
        }
        Ok(())
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
        let asset_utxo = UTXO::new(
            self.address,
            serde_json::to_vec(&asset)?,
            amount,
            privacy_flag,
        );

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

    fn generate_dummy_utxo(address: TreeHashType, amount: u128) -> UTXO {
        UTXO::new(address, vec![], amount, false)
    }

    #[test]
    fn test_new_account() {
        let account = Account::new();

        assert_eq!(account.balance, 0);
        assert!(account.key_holder.address != [0u8; 32]); // Check if the address is not empty
    }

    #[test]
    fn test_add_new_utxo_outputs() {
        let mut account = Account::new();
        let utxo1 = generate_dummy_utxo(account.address, 100);
        let utxo2 = generate_dummy_utxo(account.address, 200);

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

use anyhow::Result;
use common::transaction::Tag;
use k256::AffinePoint;
use log::info;
use nssa::Address;
use serde::{Deserialize, Serialize};

use crate::key_management::{
    constants_types::{CipherText, Nonce},
    ephemeral_key_holder::EphemeralKeyHolder,
    KeyChain,
};

pub type PublicKey = AffinePoint;

#[derive(Clone, Debug)]
pub struct NSSAUserData {
    pub key_holder: KeyChain,
    pub address: Address,
    pub balance: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NSSAUserDataForSerialization {
    pub key_holder: KeyChain,
    pub address: Address,
    pub balance: u64,
}

impl From<NSSAUserData> for NSSAUserDataForSerialization {
    fn from(value: NSSAUserData) -> Self {
        NSSAUserDataForSerialization {
            key_holder: value.key_holder,
            address: value.address,
            balance: value.balance,
        }
    }
}

impl From<NSSAUserDataForSerialization> for NSSAUserData {
    fn from(value: NSSAUserDataForSerialization) -> Self {
        NSSAUserData {
            key_holder: value.key_holder,
            address: value.address,
            balance: value.balance,
        }
    }
}

impl Serialize for NSSAUserData {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let account_for_serialization: NSSAUserDataForSerialization = From::from(self.clone());
        account_for_serialization.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for NSSAUserData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let account_for_serialization = <NSSAUserDataForSerialization>::deserialize(deserializer)?;
        Ok(account_for_serialization.into())
    }
}

///A strucure, which represents all the visible(public) information
///
/// known to each node about account `address`
///
/// Main usage is to encode data for other account
#[derive(Serialize, Clone)]
pub struct NSSAUserDataPublicMask {
    pub nullifier_public_key: AffinePoint,
    pub viewing_public_key: AffinePoint,
    pub address: Address,
    pub balance: u64,
}

impl NSSAUserDataPublicMask {
    pub fn encrypt_data(
        ephemeral_key_holder: &EphemeralKeyHolder,
        viewing_public_key_receiver: AffinePoint,
        data: &[u8],
    ) -> (CipherText, Nonce) {
        //Using of parent NSSAUserData fuction
        NSSAUserData::encrypt_data(ephemeral_key_holder, viewing_public_key_receiver, data)
    }

    pub fn make_tag(&self) -> Tag {
        self.address.value()[0]
    }
}

impl NSSAUserData {
    pub fn new() -> Self {
        let key_holder = KeyChain::new_os_random();
        let public_key =
            nssa::PublicKey::new_from_private_key(key_holder.get_pub_account_signing_key());
        let address = nssa::Address::from(&public_key);
        let balance = 0;

        Self {
            key_holder,
            address,
            balance,
        }
    }

    pub fn new_with_balance(balance: u64) -> Self {
        let key_holder = KeyChain::new_os_random();
        let public_key =
            nssa::PublicKey::new_from_private_key(key_holder.get_pub_account_signing_key());
        let address = nssa::Address::from(&public_key);

        Self {
            key_holder,
            address,
            balance,
        }
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

    pub fn update_public_balance(&mut self, new_balance: u64) {
        self.balance = new_balance;
    }

    pub fn log(&self) {
        info!("Keys generated");
        info!("NSSAUserData address is {:?}", hex::encode(self.address));
        info!("NSSAUserData balance is {:?}", self.balance);
    }

    pub fn make_tag(&self) -> Tag {
        self.address.value()[0]
    }

    ///Produce account public mask
    pub fn make_account_public_mask(&self) -> NSSAUserDataPublicMask {
        NSSAUserDataPublicMask {
            nullifier_public_key: self.key_holder.nullifer_public_key,
            viewing_public_key: self.key_holder.viewing_public_key,
            address: self.address,
            balance: self.balance,
        }
    }
}

impl Default for NSSAUserData {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_account() {
        let account = NSSAUserData::new();

        assert_eq!(account.balance, 0);
    }

    #[test]
    fn test_update_public_balance() {
        let mut account = NSSAUserData::new();
        account.update_public_balance(500);

        assert_eq!(account.balance, 500);
    }

    #[test]
    fn accounts_accounts_mask_tag_consistency() {
        let account = NSSAUserData::new();

        let account_mask = account.make_account_public_mask();

        assert_eq!(account.make_tag(), account_mask.make_tag());
    }
}

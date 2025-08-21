use std::collections::HashMap;

use anyhow::Result;
use k256::AffinePoint;
use serde::{Deserialize, Serialize};

use crate::key_management::{
    constants_types::{CipherText, Nonce},
    ephemeral_key_holder::EphemeralKeyHolder,
    KeyChain,
};

pub type PublicKey = AffinePoint;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NSSAUserData {
    pub key_holder: KeyChain,
    ///Map for all users accounts
    pub accounts: HashMap<nssa::Address, (nssa::PrivateKey, nssa_core::account::Account)>,
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

    //ToDo: Part of a private keys update
    // pub fn make_tag(&self) -> Tag {
    //     self.address.value()[0]
    // }
}

impl NSSAUserData {
    pub fn new() -> Self {
        let key_holder = KeyChain::new_os_random();

        Self {
            key_holder,
            accounts: HashMap::new(),
        }
    }

    pub fn new_with_accounts(
        accounts_data: HashMap<nssa::Address, (nssa::PrivateKey, nssa_core::account::Account)>,
    ) -> Self {
        let key_holder = KeyChain::new_os_random();

        Self {
            key_holder,
            accounts: accounts_data,
        }
    }

    pub fn generate_new_account(&mut self) -> nssa::Address {
        let private_key = nssa::PrivateKey::new_os_random();
        let address = nssa::Address::from(&nssa::PublicKey::new_from_private_key(&private_key));
        let account = nssa_core::account::Account::default();

        self.accounts.insert(address, (private_key, account));

        address
    }

    pub fn get_account_balance(&self, address: &nssa::Address) -> u128 {
        self.accounts
            .get(address)
            .map(|(_, acc)| acc.balance)
            .unwrap_or(0)
    }

    pub fn get_account(&self, address: &nssa::Address) -> Option<&nssa_core::account::Account> {
        self.accounts.get(address).map(|(_, acc)| acc)
    }

    pub fn get_account_signing_key(&self, address: &nssa::Address) -> Option<&nssa::PrivateKey> {
        self.accounts.get(address).map(|(key, _)| key)
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

    pub fn update_account_balance(&mut self, address: nssa::Address, new_balance: u128) {
        self.accounts
            .entry(address)
            .and_modify(|(_, acc)| acc.balance = new_balance);
    }

    pub fn increment_account_nonce(&mut self, address: nssa::Address) {
        self.accounts
            .entry(address)
            .and_modify(|(_, acc)| acc.nonce += 1);
    }

    //ToDo: Part of a private keys update
    // pub fn make_tag(&self) -> Tag {
    //     self.address.value()[0]
    // }

    ///Produce account public mask
    pub fn make_account_public_mask(&self) -> NSSAUserDataPublicMask {
        NSSAUserDataPublicMask {
            nullifier_public_key: self.key_holder.nullifer_public_key,
            viewing_public_key: self.key_holder.viewing_public_key,
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
        let mut user_data = NSSAUserData::new();

        let addr = user_data.generate_new_account();

        assert_eq!(user_data.get_account_balance(&addr), 0);
    }

    #[test]
    fn test_update_balance() {
        let mut user_data = NSSAUserData::new();

        let address = user_data.generate_new_account();

        user_data.update_account_balance(address, 500);

        assert_eq!(user_data.get_account_balance(&address), 500);
    }

    //ToDo: Part of a private keys update
    // #[test]
    // fn accounts_accounts_mask_tag_consistency() {
    //     let account = NSSAUserData::new();

    //     let account_mask = account.make_account_public_mask();

    //     assert_eq!(account.make_tag(), account_mask.make_tag());
    // }
}

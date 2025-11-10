use std::collections::HashMap;

use anyhow::Result;
use k256::AffinePoint;
use serde::{Deserialize, Serialize};

use crate::key_management::{
    KeyChain,
    key_tree::{KeyTreePrivate, KeyTreePublic, chain_index::ChainIndex},
    secret_holders::SeedHolder,
};

pub type PublicKey = AffinePoint;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NSSAUserData {
    ///Default public accounts
    pub default_pub_account_signing_keys: HashMap<nssa::Address, nssa::PrivateKey>,
    ///Default private accounts
    pub default_user_private_accounts:
        HashMap<nssa::Address, (KeyChain, nssa_core::account::Account)>,
    ///Mnemonic passphrase
    pub password: String,
    /// Tree of public keys
    pub public_key_tree: KeyTreePublic,
    /// Tree of private keys
    pub private_key_tree: KeyTreePrivate,
}

impl NSSAUserData {
    fn valid_public_key_transaction_pairing_check(
        accounts_keys_map: &HashMap<nssa::Address, nssa::PrivateKey>,
    ) -> bool {
        let mut check_res = true;
        for (addr, key) in accounts_keys_map {
            let expected_addr = nssa::Address::from(&nssa::PublicKey::new_from_private_key(key));
            if &expected_addr != addr {
                println!("{}, {}", expected_addr, addr);
                check_res = false;
            }
        }
        check_res
    }

    fn valid_private_key_transaction_pairing_check(
        accounts_keys_map: &HashMap<nssa::Address, (KeyChain, nssa_core::account::Account)>,
    ) -> bool {
        let mut check_res = true;
        for (addr, (key, _)) in accounts_keys_map {
            let expected_addr = nssa::Address::from(&key.nullifer_public_key);
            if expected_addr != *addr {
                println!("{}, {}", expected_addr, addr);
                check_res = false;
            }
        }
        check_res
    }

    pub fn new_with_accounts(
        default_accounts_keys: HashMap<nssa::Address, nssa::PrivateKey>,
        default_accounts_key_chains: HashMap<
            nssa::Address,
            (KeyChain, nssa_core::account::Account),
        >,
        public_key_tree: KeyTreePublic,
        private_key_tree: KeyTreePrivate,
    ) -> Result<Self> {
        if !Self::valid_public_key_transaction_pairing_check(&default_accounts_keys) {
            anyhow::bail!(
                "Key transaction pairing check not satisfied, there is addresses, which is not derived from keys"
            );
        }

        if !Self::valid_private_key_transaction_pairing_check(&default_accounts_key_chains) {
            anyhow::bail!(
                "Key transaction pairing check not satisfied, there is addresses, which is not derived from keys"
            );
        }

        Ok(Self {
            default_pub_account_signing_keys: default_accounts_keys,
            default_user_private_accounts: default_accounts_key_chains,
            public_key_tree,
            private_key_tree,
            password: "mnemonic".to_string(),
        })
    }

    pub fn new_with_accounts_and_password(
        default_accounts_keys: HashMap<nssa::Address, nssa::PrivateKey>,
        default_accounts_key_chains: HashMap<
            nssa::Address,
            (KeyChain, nssa_core::account::Account),
        >,
        public_key_tree: KeyTreePublic,
        private_key_tree: KeyTreePrivate,
        password: String,
    ) -> Result<Self> {
        if !Self::valid_public_key_transaction_pairing_check(&default_accounts_keys) {
            anyhow::bail!(
                "Key transaction pairing check not satisfied, there is addresses, which is not derived from keys"
            );
        }

        if !Self::valid_private_key_transaction_pairing_check(&default_accounts_key_chains) {
            anyhow::bail!(
                "Key transaction pairing check not satisfied, there is addresses, which is not derived from keys"
            );
        }

        Ok(Self {
            default_pub_account_signing_keys: default_accounts_keys,
            default_user_private_accounts: default_accounts_key_chains,
            public_key_tree,
            private_key_tree,
            password,
        })
    }

    /// Generated new private key for public transaction signatures
    ///
    /// Returns the address of new account
    pub fn generate_new_public_transaction_private_key(
        &mut self,
        parent_cci: ChainIndex,
    ) -> nssa::Address {
        self.public_key_tree.generate_new_node(parent_cci).unwrap()
    }

    /// Returns the signing key for public transaction signatures
    pub fn get_pub_account_signing_key(
        &self,
        address: &nssa::Address,
    ) -> Option<&nssa::PrivateKey> {
        //First seek in defaults
        if let Some(key) = self.default_pub_account_signing_keys.get(address) {
            Some(key)
        //Then seek in tree
        } else {
            self.public_key_tree
                .get_node(*address)
                .and_then(|chain_keys| Some(chain_keys.into()))
        }
    }

    /// Generated new private key for privacy preserving transactions
    ///
    /// Returns the address of new account
    pub fn generate_new_privacy_preserving_transaction_key_chain(
        &mut self,
        parent_cci: ChainIndex,
    ) -> nssa::Address {
        self.private_key_tree.generate_new_node(parent_cci).unwrap()
    }

    /// Returns the signing key for public transaction signatures
    pub fn get_private_account(
        &self,
        address: &nssa::Address,
    ) -> Option<&(KeyChain, nssa_core::account::Account)> {
        //First seek in defaults
        if let Some(key) = self.default_user_private_accounts.get(address) {
            Some(key)
        //Then seek in tree
        } else {
            self.private_key_tree
                .get_node(*address)
                .and_then(|chain_keys| Some(chain_keys.into()))
        }
    }

    /// Returns the signing key for public transaction signatures
    pub fn get_private_account_mut(
        &mut self,
        address: &nssa::Address,
    ) -> Option<&mut (KeyChain, nssa_core::account::Account)> {
        //First seek in defaults
        if let Some(key) = self.default_user_private_accounts.get_mut(address) {
            Some(key)
        //Then seek in tree
        } else {
            self.private_key_tree
                .get_node_mut(*address)
                .and_then(|chain_keys| Some(chain_keys.into()))
        }
    }
}

impl Default for NSSAUserData {
    fn default() -> Self {
        Self::new_with_accounts(
            HashMap::new(),
            HashMap::new(),
            KeyTreePublic::new(&SeedHolder::new_mnemonic("default".to_string())),
            KeyTreePrivate::new(&SeedHolder::new_mnemonic("default".to_string())),
        )
        .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_account() {
        let mut user_data = NSSAUserData::default();

        let addr_pub = user_data.generate_new_public_transaction_private_key(ChainIndex::root());
        let addr_private =
            user_data.generate_new_privacy_preserving_transaction_key_chain(ChainIndex::root());

        let is_private_key_generated = user_data.get_pub_account_signing_key(&addr_pub).is_some();

        assert!(is_private_key_generated);

        let is_key_chain_generated = user_data.get_private_account(&addr_private).is_some();

        assert!(is_key_chain_generated);

        let addr_private_str = addr_private.to_string();
        println!("{addr_private_str:#?}");
        let key_chain = &user_data.get_private_account(&addr_private).unwrap().0;
        println!("{key_chain:#?}");
    }
}

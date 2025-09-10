use std::collections::HashMap;

use anyhow::Result;
use k256::AffinePoint;
use serde::{Deserialize, Serialize};

use crate::key_management::KeyChain;

pub type PublicKey = AffinePoint;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NSSAUserData {
    ///Map for all user public accounts
    pub pub_account_signing_keys: HashMap<nssa::Address, nssa::PrivateKey>,
    ///Map for all user private accounts
    user_private_accounts: HashMap<nssa::Address, KeyChain>,
}

impl NSSAUserData {
    fn valid_public_key_transaction_pairing_check(
        accounts_keys_map: &HashMap<nssa::Address, nssa::PrivateKey>,
    ) -> bool {
        let mut check_res = true;
        for (addr, key) in accounts_keys_map {
            if &nssa::Address::from(&nssa::PublicKey::new_from_private_key(key)) != addr {
                check_res = false;
            }
        }
        check_res
    }

    fn valid_private_key_transaction_pairing_check(
        accounts_keys_map: &HashMap<nssa::Address, KeyChain>,
    ) -> bool {
        let mut check_res = true;
        for (addr, key) in accounts_keys_map {
            if nssa::Address::new(key.produce_user_address()) != *addr {
                check_res = false;
            }
        }
        check_res
    }

    pub fn new_with_accounts(
        accounts_keys: HashMap<nssa::Address, nssa::PrivateKey>,
        accounts_key_chains: HashMap<nssa::Address, KeyChain>,
    ) -> Result<Self> {
        if !Self::valid_public_key_transaction_pairing_check(&accounts_keys) {
            anyhow::bail!(
                "Key transaction pairing check not satisfied, there is addresses, which is not derived from keys"
            );
        }

        if !Self::valid_private_key_transaction_pairing_check(&accounts_key_chains) {
            anyhow::bail!(
                "Key transaction pairing check not satisfied, there is addresses, which is not derived from keys"
            );
        }

        Ok(Self {
            pub_account_signing_keys: accounts_keys,
            user_private_accounts: accounts_key_chains,
        })
    }

    /// Generated new private key for public transaction signatures
    ///
    /// Returns the address of new account
    pub fn generate_new_public_transaction_private_key(&mut self) -> nssa::Address {
        let private_key = nssa::PrivateKey::new_os_random();
        let address = nssa::Address::from(&nssa::PublicKey::new_from_private_key(&private_key));

        self.pub_account_signing_keys.insert(address, private_key);

        address
    }

    /// Returns the signing key for public transaction signatures
    pub fn get_pub_account_signing_key(
        &self,
        address: &nssa::Address,
    ) -> Option<&nssa::PrivateKey> {
        self.pub_account_signing_keys.get(address)
    }

    /// Generated new private key for privacy preserving transactions
    ///
    /// Returns the address of new account
    pub fn generate_new_privacy_preserving_transaction_key_chain(&mut self) -> nssa::Address {
        let key_chain = KeyChain::new_os_random();
        let address = nssa::Address::new(key_chain.produce_user_address());

        self.user_private_accounts.insert(address, key_chain);

        address
    }

    /// Returns the signing key for public transaction signatures
    pub fn get_private_account_key_chain(&self, address: &nssa::Address) -> Option<&KeyChain> {
        self.user_private_accounts.get(address)
    }
}

impl Default for NSSAUserData {
    fn default() -> Self {
        //Safe unwrap as maps are empty
        Self::new_with_accounts(HashMap::default(), HashMap::default()).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_account() {
        let mut user_data = NSSAUserData::default();

        let addr_pub = user_data.generate_new_public_transaction_private_key();
        let addr_private = user_data.generate_new_privacy_preserving_transaction_key_chain();

        let is_private_key_generated = user_data.get_pub_account_signing_key(&addr_pub).is_some();

        assert!(is_private_key_generated);

        let is_key_chain_generated = user_data
            .get_private_account_key_chain(&addr_private)
            .is_some();

        assert!(is_key_chain_generated);
    }
}

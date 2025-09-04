use std::collections::HashMap;

use anyhow::Result;
use k256::AffinePoint;
use serde::{Deserialize, Serialize};

use crate::key_management::KeyChain;

pub type PublicKey = AffinePoint;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NSSAUserData {
    pub key_holder: KeyChain,
}

impl NSSAUserData {
    pub fn new() -> Self {
        let key_holder = KeyChain::new_os_random();

        Self { key_holder }
    }

    fn valid_key_transaction_pairing_check(
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

    pub fn new_with_accounts(
        accounts_keys: HashMap<nssa::Address, nssa::PrivateKey>,
    ) -> Result<Self> {
        if !Self::valid_key_transaction_pairing_check(&accounts_keys) {
            anyhow::bail!(
                "Key transaction pairing check not satisfied, there is addresses, which is not derived from keys"
            );
        }

        let key_holder = KeyChain::new_os_random_with_accounts(accounts_keys);

        Ok(Self { key_holder })
    }

    pub fn generate_new_account(&mut self) -> nssa::Address {
        self.key_holder.generate_new_private_key()
    }

    pub fn get_account_signing_key(&self, address: &nssa::Address) -> Option<&nssa::PrivateKey> {
        self.key_holder.get_pub_account_signing_key(address)
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

        let _addr = user_data.generate_new_account();
    }
}

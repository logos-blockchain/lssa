use accounts::account_core::{AccountAddress, PublicKey};
use k256::AffinePoint;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct AccountPublicData {
    pub nullifier_public_key: PublicKey,
    pub viewing_public_key: PublicKey,
    pub address: AccountAddress,
}

impl AccountPublicData {
    pub fn from_raw(
        address: AccountAddress,
        nullifier_public_key: Vec<u8>,
        viewing_public_key: Vec<u8>,
    ) -> Self {
        Self {
            nullifier_public_key: serde_json::from_slice::<AffinePoint>(&nullifier_public_key)
                .unwrap(),
            viewing_public_key: serde_json::from_slice::<AffinePoint>(&viewing_public_key).unwrap(),
            address,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SequencerAccountsStore {
    pub accounts: HashMap<AccountAddress, AccountPublicData>,
}

impl SequencerAccountsStore {
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
        }
    }

    pub fn register_account(&mut self, account_pub_data: AccountPublicData) {
        self.accounts
            .insert(account_pub_data.address, account_pub_data);
    }

    pub fn unregister_account(&mut self, account_addr: AccountAddress) {
        self.accounts.remove(&account_addr);
    }
}

impl Default for SequencerAccountsStore {
    fn default() -> Self {
        Self::new()
    }
}

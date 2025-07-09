use accounts::account_core::AccountAddress;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountPublicData {
    pub balance: u64,
    pub address: AccountAddress,
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

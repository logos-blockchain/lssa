use accounts::account_core::AccountAddress;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AccountPublicData {
    pub balance: u64,
    pub address: AccountAddress,
}

impl AccountPublicData {
    pub fn new(address: AccountAddress) -> Self {
        Self {
            balance: 0,
            address,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SequencerAccountsStore {
    accounts: HashMap<AccountAddress, AccountPublicData>,
}

impl SequencerAccountsStore {
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
        }
    }

    pub fn register_account(&mut self, account_addr: AccountAddress) {
        self.accounts
            .insert(account_addr, AccountPublicData::new(account_addr));
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

use accounts::account_core::{Account, AccountAddress};
use std::collections::HashMap;

pub struct NodeAccountsStore {
    pub accounts: HashMap<AccountAddress, Account>,
}

impl NodeAccountsStore {
    pub fn new() -> Self {
        Self {
            accounts: HashMap::new(),
        }
    }

    pub fn register_account(&mut self, account: Account) {
        self.accounts.insert(account.address, account);
    }

    pub fn unregister_account(&mut self, account_addr: AccountAddress) {
        self.accounts.remove(&account_addr);
    }
}

impl Default for NodeAccountsStore {
    fn default() -> Self {
        Self::new()
    }
}

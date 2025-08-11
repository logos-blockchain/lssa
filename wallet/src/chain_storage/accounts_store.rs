use accounts::account_core::{address::AccountAddress, Account};
use std::collections::HashMap;

pub struct WalletAccountsStore {
    pub accounts: HashMap<AccountAddress, Account>,
}

impl WalletAccountsStore {
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

impl Default for WalletAccountsStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use accounts::account_core::Account;
    /// Helper function to create a sample account
    fn create_sample_account(balance: u64) -> Account {
        Account::new_with_balance(balance)
    }

    fn pad_to_32(slice: &[u8]) -> [u8; 32] {
        let mut padded = [0u8; 32];
        let len = slice.len().min(32);
        padded[..len].copy_from_slice(&slice[..len]);
        padded
    }

    #[test]
    fn test_create_empty_store() {
        let store = WalletAccountsStore::new();
        assert!(store.accounts.is_empty());
    }

    #[test]
    fn test_register_account() {
        let mut store = WalletAccountsStore::new();

        let account = create_sample_account(100);
        let account_addr = account.address;

        store.register_account(account);

        assert_eq!(store.accounts.len(), 1);
        let stored_account = store.accounts.get(&account_addr).unwrap();
        assert_eq!(stored_account.balance, 100);
    }

    #[test]
    fn test_unregister_account() {
        let mut store = WalletAccountsStore::new();

        let account = create_sample_account(100);
        let account_addr = account.address;
        store.register_account(account);

        assert_eq!(store.accounts.len(), 1);

        store.unregister_account(account_addr);
        assert!(store.accounts.is_empty());
    }

    #[test]
    fn test_unregister_nonexistent_account() {
        let mut store = WalletAccountsStore::new();

        let account_addr: [u8; 32] = pad_to_32("nonexistent".to_string().as_bytes());
        store.unregister_account(account_addr);

        assert!(store.accounts.is_empty());
    }

    #[test]
    fn test_register_multiple_accounts() {
        let mut store = WalletAccountsStore::new();

        let account1 = create_sample_account(100);
        let account2 = create_sample_account(200);

        let address_1 = account1.address;
        let address_2 = account2.address;

        store.register_account(account1);
        store.register_account(account2);

        assert_eq!(store.accounts.len(), 2);

        let stored_account1 = store.accounts.get(&address_1).unwrap();
        let stored_account2 = store.accounts.get(&address_2).unwrap();

        assert_eq!(stored_account1.balance, 100);
        assert_eq!(stored_account2.balance, 200);
    }
}

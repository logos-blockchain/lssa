use accounts::account_core::address::AccountAddress;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AccountPublicData {
    pub balance: u64,
    pub address: AccountAddress,
    nonce: u64,
}

impl AccountPublicData {
    pub fn new(address: AccountAddress) -> Self {
        Self {
            balance: 0,
            nonce: 0,
            address,
        }
    }

    fn new_with_balance(address: AccountAddress, balance: u64) -> Self {
        Self {
            balance,
            address,
            nonce: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SequencerAccountsStore {
    accounts: HashMap<AccountAddress, AccountPublicData>,
}

impl SequencerAccountsStore {
    pub fn new(initial_accounts: &[(AccountAddress, u64)]) -> Self {
        let mut accounts = HashMap::new();

        for (account_addr, balance) in initial_accounts {
            accounts.insert(
                *account_addr,
                AccountPublicData::new_with_balance(*account_addr, *balance),
            );
        }

        Self { accounts }
    }

    ///Register new account in accounts store
    ///
    ///Starts with zero public balance
    pub fn register_account(&mut self, account_addr: AccountAddress) {
        self.accounts
            .insert(account_addr, AccountPublicData::new(account_addr));
    }

    ///Check, if `account_addr` present in account store
    pub fn contains_account(&self, account_addr: &AccountAddress) -> bool {
        self.accounts.contains_key(account_addr)
    }

    ///Check `account_addr` balance,
    ///
    ///returns 0, if account address not found
    pub fn get_account_balance(&self, account_addr: &AccountAddress) -> u64 {
        self.accounts
            .get(account_addr)
            .map(|acc| acc.balance)
            .unwrap_or(0)
    }

    pub fn get_account_nonce(&self, account_addr: &AccountAddress) -> u64 {
        self.accounts
            .get(account_addr)
            .map(|acc| acc.nonce)
            .unwrap_or(0)
    }

    ///Update `account_addr` balance,
    ///
    /// returns 0, if account address not found, otherwise returns previous balance
    ///
    /// Also, if account was not previously found, sets it with zero balance
    pub fn set_account_balance(&mut self, account_addr: &AccountAddress, new_balance: u64) -> u64 {
        let acc_data = self.accounts.get_mut(account_addr);

        if let Some(acc_data) = acc_data {
            let old_balance = acc_data.balance;

            acc_data.balance = new_balance;

            old_balance
        } else {
            self.register_account(*account_addr);

            0
        }
    }

    ///Update `account_addr` nonce,
    ///
    /// Returns previous nonce
    pub fn increase_nonce(&mut self, account_addr: &AccountAddress) -> u64 {
        if let Some(acc_data) = self.accounts.get_mut(account_addr) {
            let old_nonce = acc_data.nonce;
            acc_data.nonce += 1;
            old_nonce
        } else {
            self.register_account(*account_addr);
            self.increase_nonce(account_addr)
        }
    }

    ///Remove account from storage
    ///
    /// Fails, if `balance` is != 0
    ///
    /// Returns `Option<AccountAddress>` which is `None` if `account_addr` vere not present in store
    pub fn unregister_account(
        &mut self,
        account_addr: AccountAddress,
    ) -> Result<Option<AccountAddress>> {
        if self.get_account_balance(&account_addr) == 0 {
            Ok(self.accounts.remove(&account_addr).map(|data| data.address))
        } else {
            anyhow::bail!("Chain consistency violation: It is forbidden to remove account with nonzero balance");
        }
    }

    ///Number of accounts present in store
    pub fn len(&self) -> usize {
        self.accounts.len()
    }

    ///Is accounts store empty
    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }
}

impl Default for SequencerAccountsStore {
    fn default() -> Self {
        Self::new(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_balance_account_data_creation() {
        let new_acc = AccountPublicData::new([1; 32]);

        assert_eq!(new_acc.balance, 0);
        assert_eq!(new_acc.address, [1; 32]);
    }

    #[test]
    fn test_zero_nonce_account_data_creation() {
        let new_acc = AccountPublicData::new([1; 32]);

        assert_eq!(new_acc.nonce, 0);
    }

    #[test]
    fn test_non_zero_balance_account_data_creation() {
        let new_acc = AccountPublicData::new_with_balance([1; 32], 10);

        assert_eq!(new_acc.balance, 10);
        assert_eq!(new_acc.address, [1; 32]);
    }

    #[test]
    fn test_zero_nonce_account_data_creation_with_balance() {
        let new_acc = AccountPublicData::new_with_balance([1; 32], 10);

        assert_eq!(new_acc.nonce, 0);
    }

    #[test]
    fn default_account_sequencer_store() {
        let seq_acc_store = SequencerAccountsStore::default();

        assert!(seq_acc_store.accounts.is_empty());
    }

    #[test]
    fn account_sequencer_store_register_acc() {
        let mut seq_acc_store = SequencerAccountsStore::default();

        seq_acc_store.register_account([1; 32]);

        assert!(seq_acc_store.contains_account(&[1; 32]));

        let acc_balance = seq_acc_store.get_account_balance(&[1; 32]);

        assert_eq!(acc_balance, 0);
    }

    #[test]
    fn account_sequencer_store_unregister_acc_not_present() {
        let mut seq_acc_store = SequencerAccountsStore::default();

        seq_acc_store.register_account([1; 32]);

        let rem_res = seq_acc_store.unregister_account([2; 32]).unwrap();

        assert!(rem_res.is_none());
    }

    #[test]
    fn account_sequencer_store_unregister_acc_not_zero_balance() {
        let mut seq_acc_store = SequencerAccountsStore::new(&[([1; 32], 12), ([2; 32], 100)]);

        let rem_res = seq_acc_store.unregister_account([1; 32]);

        assert!(rem_res.is_err());
    }

    #[test]
    fn account_sequencer_store_unregister_acc() {
        let mut seq_acc_store = SequencerAccountsStore::default();

        seq_acc_store.register_account([1; 32]);

        assert!(seq_acc_store.contains_account(&[1; 32]));

        seq_acc_store.unregister_account([1; 32]).unwrap().unwrap();

        assert!(!seq_acc_store.contains_account(&[1; 32]));
    }

    #[test]
    fn account_sequencer_store_with_preset_accounts_1() {
        let seq_acc_store = SequencerAccountsStore::new(&[([1; 32], 12), ([2; 32], 100)]);

        assert!(seq_acc_store.contains_account(&[1; 32]));
        assert!(seq_acc_store.contains_account(&[2; 32]));

        let acc_balance = seq_acc_store.get_account_balance(&[1; 32]);

        assert_eq!(acc_balance, 12);

        let acc_balance = seq_acc_store.get_account_balance(&[2; 32]);

        assert_eq!(acc_balance, 100);
    }

    #[test]
    fn account_sequencer_store_with_preset_accounts_2() {
        let seq_acc_store =
            SequencerAccountsStore::new(&[([6; 32], 120), ([7; 32], 15), ([8; 32], 10)]);

        assert!(seq_acc_store.contains_account(&[6; 32]));
        assert!(seq_acc_store.contains_account(&[7; 32]));
        assert!(seq_acc_store.contains_account(&[8; 32]));

        let acc_balance = seq_acc_store.get_account_balance(&[6; 32]);

        assert_eq!(acc_balance, 120);

        let acc_balance = seq_acc_store.get_account_balance(&[7; 32]);

        assert_eq!(acc_balance, 15);

        let acc_balance = seq_acc_store.get_account_balance(&[8; 32]);

        assert_eq!(acc_balance, 10);
    }

    #[test]
    fn account_sequencer_store_fetch_unknown_account() {
        let seq_acc_store =
            SequencerAccountsStore::new(&[([6; 32], 120), ([7; 32], 15), ([8; 32], 10)]);

        let acc_balance = seq_acc_store.get_account_balance(&[9; 32]);

        assert_eq!(acc_balance, 0);
    }

    #[test]
    fn account_sequencer_store_is_empty_test() {
        let seq_acc_store = SequencerAccountsStore::default();

        assert!(seq_acc_store.is_empty());
    }

    #[test]
    fn account_sequencer_store_set_balance_to_unknown_account() {
        let mut seq_acc_store = SequencerAccountsStore::default();

        let ret = seq_acc_store.set_account_balance(&[1; 32], 100);

        assert_eq!(ret, 0);
        assert!(seq_acc_store.contains_account(&[1; 32]));
        assert_eq!(seq_acc_store.get_account_balance(&[1; 32]), 0);
    }

    #[test]
    fn test_increase_nonce() {
        let mut account_store = SequencerAccountsStore::default();
        let address = [1; 32];
        let first_nonce = account_store.increase_nonce(&address);
        assert_eq!(first_nonce, 0);
        let second_nonce = account_store.increase_nonce(&address);
        assert_eq!(second_nonce, 1);
    }
}

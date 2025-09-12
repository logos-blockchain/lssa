use crate::program::ProgramId;
use serde::{Deserialize, Serialize};

pub type Nonce = u128;
type Data = Vec<u8>;

/// Account to be used both in public and private contexts
#[derive(Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
#[cfg_attr(any(feature = "host", test), derive(Debug))]
pub struct Account {
    pub program_owner: ProgramId,
    pub balance: u128,
    pub data: Data,
    pub nonce: Nonce,
}

/// A fingerprint of the owner of an account. This can be, for example, an `Address` in case the account
/// is public, or a `NullifierPublicKey` in case the account is private.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(any(feature = "host", test), derive(Debug))]
pub struct AccountId([u8; 32]);
impl AccountId {
    pub fn new(value: [u8; 32]) -> Self {
        Self(value)
    }
}

#[derive(Serialize, Deserialize, Clone)]
#[cfg_attr(any(feature = "host", test), derive(Debug, PartialEq, Eq))]
pub struct AccountWithMetadata {
    pub account: Account,
    pub is_authorized: bool,
    pub account_id: AccountId,
}

#[cfg(feature = "host")]
impl AccountWithMetadata {
    pub fn new(account: Account, is_authorized: bool, account_id: impl Into<AccountId>) -> Self {
        Self {
            account,
            is_authorized,
            account_id: account_id.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::program::DEFAULT_PROGRAM_ID;

    use super::*;

    #[test]
    fn test_zero_balance_account_data_creation() {
        let new_acc = Account::default();

        assert_eq!(new_acc.balance, 0);
    }

    #[test]
    fn test_zero_nonce_account_data_creation() {
        let new_acc = Account::default();

        assert_eq!(new_acc.nonce, 0);
    }

    #[test]
    fn test_empty_data_account_data_creation() {
        let new_acc = Account::default();

        assert!(new_acc.data.is_empty());
    }

    #[test]
    fn test_default_program_owner_account_data_creation() {
        let new_acc = Account::default();

        assert_eq!(new_acc.program_owner, DEFAULT_PROGRAM_ID);
    }

    #[test]
    fn test_account_with_metadata_constructor() {
        let account = Account {
            program_owner: [1, 2, 3, 4, 5, 6, 7, 8],
            balance: 1337,
            data: b"testing_account_with_metadata_constructor".to_vec(),
            nonce: 0xdeadbeef,
        };
        let fingerprint = AccountId::new([8; 32]);
        let new_acc_with_metadata =
            AccountWithMetadata::new(account.clone(), true, fingerprint.clone());
        assert_eq!(new_acc_with_metadata.account, account);
        assert!(new_acc_with_metadata.is_authorized);
        assert_eq!(new_acc_with_metadata.account_id, fingerprint);
    }
}

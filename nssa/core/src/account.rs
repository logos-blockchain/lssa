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

#[derive(Serialize, Deserialize, Clone)]
#[cfg_attr(any(feature = "host", test), derive(Debug, PartialEq, Eq))]
pub struct AccountWithMetadata {
    pub account: Account,
    pub is_authorized: bool,
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
}

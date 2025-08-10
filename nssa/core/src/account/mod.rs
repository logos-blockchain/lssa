use serde::{Deserialize, Serialize};

use crate::program::ProgramId;

pub type Nonce = u128;
type Data = Vec<u8>;

/// Account to be used both in public and private contexts
#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Account {
    pub program_owner: ProgramId,
    pub balance: u128,
    pub data: Data,
    pub nonce: Nonce,
}

#[derive(Clone, Serialize, Deserialize)]
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

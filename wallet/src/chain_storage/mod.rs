use std::collections::HashMap;

use anyhow::Result;
use key_protocol::key_protocol_core::NSSAUserData;

use crate::config::{PersistentAccountData, WalletConfig};

pub struct WalletChainStore {
    pub user_data: NSSAUserData,
    pub wallet_config: WalletConfig,
}

impl WalletChainStore {
    pub fn new(config: WalletConfig) -> Result<Self> {
        let accounts_keys: HashMap<nssa::Address, nssa::PrivateKey> = config
            .initial_accounts
            .clone()
            .into_iter()
            .map(|init_acc_data| (init_acc_data.address, init_acc_data.pub_sign_key))
            .collect();

        Ok(Self {
            user_data: NSSAUserData::new_with_accounts(accounts_keys)?,
            wallet_config: config,
        })
    }

    pub(crate) fn insert_account_data(&mut self, acc_data: PersistentAccountData) {
        self.user_data
            .key_holder
            .pub_account_signing_keys
            .insert(acc_data.address, acc_data.pub_sign_key);
    }
}

#[cfg(test)]
mod tests {
    use crate::config::InitialAccountData;

    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn create_initial_accounts() -> Vec<InitialAccountData> {
        let initial_acc1 = serde_json::from_str(r#"{
            "address": "1b84c5567b126440995d3ed5aaba0565d71e1834604819ff9c17f5e9d5dd078f",
            "pub_sign_key": [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
            "account": {
                "program_owner": [0,0,0,0,0,0,0,0],
                "balance": 100,
                "nonce": 0,
                "data": []
            }
        }"#).unwrap();

        let initial_acc2 = serde_json::from_str(r#"{
            "address": "4d4b6cd1361032ca9bd2aeb9d900aa4d45d9ead80ac9423374c451a7254d0766",
            "pub_sign_key": [2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2],
            "account": {
                "program_owner": [0,0,0,0,0,0,0,0],
                "balance": 100,
                "nonce": 0,
                "data": []
            }
        }"#).unwrap();

        let initial_accounts = vec![initial_acc1, initial_acc2];

        initial_accounts
    }

    fn create_sample_wallet_config(home: PathBuf) -> WalletConfig {
        WalletConfig {
            home,
            override_rust_log: None,
            sequencer_addr: "http://127.0.0.1".to_string(),
            seq_poll_timeout_millis: 12000,
            seq_poll_max_blocks: 5,
            seq_poll_max_retries: 10,
            seq_poll_retry_delay_millis: 500,
            initial_accounts: create_initial_accounts(),
        }
    }

    #[test]
    fn test_new_initializes_correctly() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path();

        let config = create_sample_wallet_config(path.to_path_buf());

        let _ = WalletChainStore::new(config.clone()).unwrap();
    }
}

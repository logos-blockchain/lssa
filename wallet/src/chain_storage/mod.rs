use std::collections::HashMap;

use anyhow::Result;
use common::merkle_tree_public::merkle_tree::UTXOCommitmentsMerkleTree;
use key_protocol::key_protocol_core::NSSAUserData;

use crate::config::WalletConfig;

pub struct WalletChainStore {
    pub user_data: NSSAUserData,
    pub utxo_commitments_store: UTXOCommitmentsMerkleTree,
    pub wallet_config: WalletConfig,
}

impl WalletChainStore {
    pub fn new(config: WalletConfig) -> Result<Self> {
        let accounts_data: HashMap<nssa::Address, (nssa::PrivateKey, nssa_core::account::Account)> =
            config
                .initial_accounts
                .clone()
                .into_iter()
                .map(|init_acc_data| {
                    (
                        init_acc_data.address,
                        (init_acc_data.pub_sign_key, init_acc_data.account),
                    )
                })
                .collect();

        let utxo_commitments_store = UTXOCommitmentsMerkleTree::new(vec![]);

        Ok(Self {
            user_data: NSSAUserData::new_with_accounts(accounts_data),
            utxo_commitments_store,
            wallet_config: config,
        })
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

        let store = WalletChainStore::new(config.clone()).unwrap();

        assert_eq!(store.user_data.accounts.len(), 2);
        assert_eq!(
            store.utxo_commitments_store.get_root().unwrap_or([0; 32]),
            [0; 32]
        );
    }
}

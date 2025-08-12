use std::collections::{BTreeMap, HashMap};

use accounts::account_core::{address::AccountAddress, Account};
use anyhow::Result;
use common::merkle_tree_public::merkle_tree::UTXOCommitmentsMerkleTree;
use sc_core::public_context::PublicSCContext;
use serde::{Deserialize, Serialize};

use crate::config::WalletConfig;

pub mod accounts_store;

#[derive(Deserialize, Serialize)]
pub struct AccMap {
    pub acc_map: HashMap<String, Account>,
}

impl From<HashMap<[u8; 32], Account>> for AccMap {
    fn from(value: HashMap<[u8; 32], Account>) -> Self {
        AccMap {
            acc_map: value
                .into_iter()
                .map(|(key, val)| (hex::encode(key), val))
                .collect(),
        }
    }
}

impl From<AccMap> for HashMap<[u8; 32], Account> {
    fn from(value: AccMap) -> Self {
        value
            .acc_map
            .into_iter()
            .map(|(key, val)| (hex::decode(key).unwrap().try_into().unwrap(), val))
            .collect()
    }
}

pub struct WalletChainStore {
    pub acc_map: HashMap<AccountAddress, Account>,
    pub utxo_commitments_store: UTXOCommitmentsMerkleTree,
    pub wallet_config: WalletConfig,
}

impl WalletChainStore {
    pub fn new(config: WalletConfig) -> Result<Self> {
        let acc_map = HashMap::new();
        let utxo_commitments_store = UTXOCommitmentsMerkleTree::new(vec![]);

        Ok(Self {
            acc_map,
            utxo_commitments_store,
            wallet_config: config,
        })
    }

    pub fn produce_context(&self, caller: AccountAddress) -> PublicSCContext {
        let mut account_masks = BTreeMap::new();

        for (acc_addr, acc) in &self.acc_map {
            account_masks.insert(*acc_addr, acc.make_account_public_mask());
        }

        PublicSCContext {
            caller_address: caller,
            caller_balance: self.acc_map.get(&caller).unwrap().balance,
            account_masks,
            comitment_store_root: self.utxo_commitments_store.get_root().unwrap_or([0; 32]),
            commitments_tree: self.utxo_commitments_store.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use accounts::account_core::Account;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn create_initial_accounts() -> Vec<Account> {
        let initial_acc1 = serde_json::from_str(r#"{
            "address": [27, 132, 197, 86, 123, 18, 100, 64, 153, 93, 62, 213, 170, 186, 5, 101, 215, 30, 24, 52, 96, 72, 25, 255, 156, 23, 245, 233, 213, 221, 7, 143],
            "balance": 100,
            "key_holder": {
                "nullifer_public_key": "03A340BECA9FAAB444CED0140681D72EA1318B5C611704FEE017DA9836B17DB718",
                "pub_account_signing_key": [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
                "top_secret_key_holder": {
                    "secret_spending_key": "7BC46784DB1BC67825D8F029436846712BFDF9B5D79EA3AB11D39A52B9B229D4"
                },
                "utxo_secret_key_holder": {
                    "nullifier_secret_key": "BB54A8D3C9C51B82C431082D1845A74677B0EF829A11B517E1D9885DE3139506",
                    "viewing_secret_key": "AD923E92F6A5683E30140CEAB2702AFB665330C1EE4EFA70FAF29767B6B52BAF"
                },
                "viewing_public_key": "0361220C5D277E7A1709340FD31A52600C1432B9C45B9BCF88A43581D58824A8B6"
            },
            "utxos": {}
        }"#).unwrap();

        let initial_acc2 = serde_json::from_str(r#"{
            "address": [77, 75, 108, 209, 54, 16, 50, 202, 155, 210, 174, 185, 217, 0, 170, 77, 69, 217, 234, 216, 10, 201, 66, 51, 116, 196, 81, 167, 37, 77, 7, 102],
            "balance": 200,
            "key_holder": {
                "nullifer_public_key": "02172F50274DE67C4087C344F5D58E11DF761D90285B095060E0994FAA6BCDE271",
                "pub_account_signing_key": [2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2],
                "top_secret_key_holder": {
                    "secret_spending_key": "80A186737C8D38B4288A03F0F589957D9C040D79C19F3E0CC4BA80F8494E5179"
                },
                "utxo_secret_key_holder": {
                    "nullifier_secret_key": "746928E63F0984F6F4818933493CE9C067562D9CB932FDC06D82C86CDF6D7122",
                    "viewing_secret_key": "89176CF4BC9E673807643FD52110EF99D4894335AFB10D881AC0B5041FE1FCB7"
                },
                "viewing_public_key": "026072A8F83FEC3472E30CDD4767683F30B91661D25B1040AD9A5FC2E01D659F99"
            },
            "utxos": {}
        }"#).unwrap();

        let initial_accounts = vec![initial_acc1, initial_acc2];

        initial_accounts
    }

    fn create_sample_wallet_config(home: PathBuf) -> WalletConfig {
        WalletConfig {
            home,
            override_rust_log: None,
            sequencer_addr: "http://127.0.0.1".to_string(),
            seq_poll_timeout_secs: 1,
            initial_accounts: create_initial_accounts(),
        }
    }

    #[test]
    fn test_new_initializes_correctly() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path();

        let config = create_sample_wallet_config(path.to_path_buf());

        let store = WalletChainStore::new(config.clone()).unwrap();

        assert!(store.acc_map.is_empty());
        assert_eq!(
            store.utxo_commitments_store.get_root().unwrap_or([0; 32]),
            [0; 32]
        );
    }
}

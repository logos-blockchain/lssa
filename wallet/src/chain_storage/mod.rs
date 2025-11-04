use std::collections::HashMap;

use anyhow::Result;
use key_protocol::key_protocol_core::NSSAUserData;
use nssa::program::Program;

use crate::config::{InitialAccountData, PersistentAccountData, WalletConfig};

pub struct WalletChainStore {
    pub user_data: NSSAUserData,
    pub wallet_config: WalletConfig,
}

impl WalletChainStore {
    pub fn new(config: WalletConfig) -> Result<Self> {
        let mut public_init_acc_map = HashMap::new();
        let mut private_init_acc_map = HashMap::new();

        for init_acc_data in config.initial_accounts.clone() {
            match init_acc_data {
                InitialAccountData::Public(data) => {
                    public_init_acc_map.insert(data.address.parse()?, data.pub_sign_key);
                }
                InitialAccountData::Private(data) => {
                    let mut account = data.account;
                    // TODO: Program owner is only known after code is compiled and can't be set in
                    // the config. Therefore we overwrite it here on startup. Fix this when program
                    // id can be fetched from the node and queried from the wallet.
                    account.program_owner = Program::authenticated_transfer_program().id();
                    private_init_acc_map.insert(data.address.parse()?, (data.key_chain, account));
                }
            }
        }

        Ok(Self {
            user_data: NSSAUserData::new_with_accounts(public_init_acc_map, private_init_acc_map)?,
            wallet_config: config,
        })
    }

    pub fn insert_private_account_data(
        &mut self,
        addr: nssa::Address,
        account: nssa_core::account::Account,
    ) {
        println!("inserting at addres {}, this account {:?}", addr, account);
        self.user_data
            .user_private_accounts
            .entry(addr)
            .and_modify(|(_, acc)| *acc = account);
    }

    pub(crate) fn insert_account_data(&mut self, acc_data: PersistentAccountData) {
        match acc_data {
            PersistentAccountData::Public(acc_data) => {
                self.user_data
                    .pub_account_signing_keys
                    .insert(acc_data.address, acc_data.pub_sign_key);
            }
            PersistentAccountData::Private(acc_data) => {
                self.user_data
                    .user_private_accounts
                    .insert(acc_data.address, (acc_data.key_chain, acc_data.account));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::config::InitialAccountData;

    use super::*;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn create_initial_accounts() -> Vec<InitialAccountData> {
        let initial_acc1 = serde_json::from_str(
            r#"{
            "Public": {
                "address": "BLgCRDXYdQPMMWVHYRFGQZbgeHx9frkipa8GtpG2Syqy",
                "pub_sign_key": [
                    16,
                    162,
                    106,
                    154,
                    236,
                    125,
                    52,
                    184,
                    35,
                    100,
                    238,
                    174,
                    69,
                    197,
                    41,
                    77,
                    187,
                    10,
                    118,
                    75,
                    0,
                    11,
                    148,
                    238,
                    185,
                    181,
                    133,
                    17,
                    220,
                    72,
                    124,
                    77
                ]
            }
        }"#,
        )
        .unwrap();

        let initial_acc2 = serde_json::from_str(
            r#"{
            "Public": {
                "address": "Gj1mJy5W7J5pfmLRujmQaLfLMWidNxQ6uwnhb666ZwHw",
                "pub_sign_key": [
                    113,
                    121,
                    64,
                    177,
                    204,
                    85,
                    229,
                    214,
                    178,
                    6,
                    109,
                    191,
                    29,
                    154,
                    63,
                    38,
                    242,
                    18,
                    244,
                    219,
                    8,
                    208,
                    35,
                    136,
                    23,
                    127,
                    207,
                    237,
                    216,
                    169,
                    190,
                    27
                ]
            }
        }"#,
        )
        .unwrap();

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

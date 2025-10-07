use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use nssa_core::account::Nonce;
use rand::{RngCore, rngs::OsRng};
use std::{fs::File, io::BufReader, path::PathBuf, str::FromStr};

use anyhow::Result;
use key_protocol::key_protocol_core::NSSAUserData;
use nssa::Account;
use serde::Serialize;

use crate::{
    HOME_DIR_ENV_VAR,
    config::{
        PersistentAccountData, PersistentAccountDataPrivate, PersistentAccountDataPublic,
        WalletConfig,
    },
};

/// Get home dir for wallet. Env var `NSSA_WALLET_HOME_DIR` must be set before execution to succeed.
pub fn get_home() -> Result<PathBuf> {
    Ok(PathBuf::from_str(&std::env::var(HOME_DIR_ENV_VAR)?)?)
}

/// Fetch config from `NSSA_WALLET_HOME_DIR`
pub fn fetch_config() -> Result<WalletConfig> {
    let config_home = get_home()?;
    let file = File::open(config_home.join("wallet_config.json"))?;
    let reader = BufReader::new(file);

    Ok(serde_json::from_reader(reader)?)
}

/// Fetch list of accounts stored at `NSSA_WALLET_HOME_DIR/curr_accounts.json`
///
/// If file not present, it is considered as empty list of persistent accounts
pub fn fetch_persistent_accounts() -> Result<Vec<PersistentAccountData>> {
    let home = get_home()?;
    let accs_path = home.join("curr_accounts.json");

    match File::open(accs_path) {
        Ok(file) => {
            let reader = BufReader::new(file);
            Ok(serde_json::from_reader(reader)?)
        }
        Err(err) => match err.kind() {
            std::io::ErrorKind::NotFound => Ok(vec![]),
            _ => {
                anyhow::bail!("IO error {err:#?}");
            }
        },
    }
}

/// Produces a list of accounts for storage
pub fn produce_data_for_storage(user_data: &NSSAUserData) -> Vec<PersistentAccountData> {
    let mut vec_for_storage = vec![];

    for (addr, key) in &user_data.pub_account_signing_keys {
        vec_for_storage.push(
            PersistentAccountDataPublic {
                address: *addr,
                pub_sign_key: key.clone(),
            }
            .into(),
        );
    }

    for (addr, (key, acc)) in &user_data.user_private_accounts {
        vec_for_storage.push(
            PersistentAccountDataPrivate {
                address: *addr,
                account: acc.clone(),
                key_chain: key.clone(),
            }
            .into(),
        );
    }

    vec_for_storage
}

pub(crate) fn produce_random_nonces(size: usize) -> Vec<Nonce> {
    let mut result = vec![[0; 16]; size];
    result.iter_mut().for_each(|bytes| OsRng.fill_bytes(bytes));
    result.into_iter().map(Nonce::from_le_bytes).collect()
}

/// Human-readable representation of an account.
#[derive(Serialize)]
pub(crate) struct HumanReadableAccount {
    balance: u128,
    program_owner_b64: String,
    data_b64: String,
    nonce: u128,
}

impl From<Account> for HumanReadableAccount {
    fn from(account: Account) -> Self {
        let program_owner_b64 = BASE64.encode(bytemuck::cast_slice(&account.program_owner));
        let data_b64 = BASE64.encode(account.data);
        Self {
            balance: account.balance,
            program_owner_b64,
            data_b64,
            nonce: account.nonce,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_home_get_env_var() {
        unsafe {
            std::env::set_var(HOME_DIR_ENV_VAR, "/path/to/configs");
        }

        let home = get_home().unwrap();

        assert_eq!(PathBuf::from_str("/path/to/configs").unwrap(), home);

        unsafe {
            std::env::remove_var(HOME_DIR_ENV_VAR);
        }
    }
}

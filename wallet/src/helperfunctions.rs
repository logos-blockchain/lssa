use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use nssa_core::account::Nonce;
use rand::{RngCore, rngs::OsRng};
use std::{path::PathBuf, str::FromStr};
use tokio::io::AsyncReadExt;

use anyhow::Result;
use key_protocol::key_protocol_core::NSSAUserData;
use nssa::Account;
use serde::Serialize;

use crate::{
    HOME_DIR_ENV_VAR,
    config::{
        PersistentAccountDataPrivate, PersistentAccountDataPublic, PersistentStorage, WalletConfig,
    },
};

/// Get home dir for wallet. Env var `NSSA_WALLET_HOME_DIR` must be set before execution to succeed.
pub fn get_home() -> Result<PathBuf> {
    Ok(PathBuf::from_str(&std::env::var(HOME_DIR_ENV_VAR)?)?)
}

/// Fetch config from `NSSA_WALLET_HOME_DIR`
pub async fn fetch_config() -> Result<WalletConfig> {
    let config_home = get_home()?;
    let config_contents = tokio::fs::read(config_home.join("wallet_config.json")).await?;

    Ok(serde_json::from_slice(&config_contents)?)
}

/// Fetch data stored at `NSSA_WALLET_HOME_DIR/storage.json`
///
/// If file not present, it is considered as empty list of persistent accounts
pub async fn fetch_persistent_storage() -> Result<PersistentStorage> {
    let home = get_home()?;
    let accs_path = home.join("storage.json");
    let mut storage_content = vec![];

    match tokio::fs::File::open(accs_path).await {
        Ok(mut file) => {
            file.read_to_end(&mut storage_content).await?;
            Ok(serde_json::from_slice(&storage_content)?)
        }
        Err(err) => match err.kind() {
            std::io::ErrorKind::NotFound => Ok(PersistentStorage {
                accounts: vec![],
                last_synced_block: 0,
            }),
            _ => {
                anyhow::bail!("IO error {err:#?}");
            }
        },
    }
}

/// Produces data for storage
pub fn produce_data_for_storage(
    user_data: &NSSAUserData,
    last_synced_block: u64,
) -> PersistentStorage {
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

    PersistentStorage {
        accounts: vec_for_storage,
        last_synced_block,
    }
}

pub(crate) fn produce_random_nonces(size: usize) -> Vec<Nonce> {
    let mut result = vec![[0; 16]; size];
    result.iter_mut().for_each(|bytes| OsRng.fill_bytes(bytes));
    result.into_iter().map(Nonce::from_le_bytes).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressPrivacyKind {
    Public,
    Private,
}

pub(crate) fn parse_addr_with_privacy_prefix(
    addr_base58: &str,
) -> Result<(String, AddressPrivacyKind)> {
    if addr_base58.starts_with("Public/") {
        Ok((
            addr_base58.strip_prefix("Public/").unwrap().to_string(),
            AddressPrivacyKind::Public,
        ))
    } else if addr_base58.starts_with("Private/") {
        Ok((
            addr_base58.strip_prefix("Private/").unwrap().to_string(),
            AddressPrivacyKind::Private,
        ))
    } else {
        anyhow::bail!("Unsupported privacy kind, available variants is Public/ and Private/");
    }
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

    #[test]
    fn test_addr_parse_with_privacy() {
        let addr_base58 = "Public/BLgCRDXYdQPMMWVHYRFGQZbgeHx9frkipa8GtpG2Syqy";
        let (_, addr_kind) = parse_addr_with_privacy_prefix(addr_base58).unwrap();

        assert_eq!(addr_kind, AddressPrivacyKind::Public);

        let addr_base58 = "Private/BLgCRDXYdQPMMWVHYRFGQZbgeHx9frkipa8GtpG2Syqy";
        let (_, addr_kind) = parse_addr_with_privacy_prefix(addr_base58).unwrap();

        assert_eq!(addr_kind, AddressPrivacyKind::Private);

        let addr_base58 = "asdsada/BLgCRDXYdQPMMWVHYRFGQZbgeHx9frkipa8GtpG2Syqy";
        assert!(parse_addr_with_privacy_prefix(addr_base58).is_err());
    }
}

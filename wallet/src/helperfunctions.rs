use std::{path::PathBuf, str::FromStr};

use anyhow::Result;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use key_protocol::key_protocol_core::NSSAUserData;
use nssa::Account;
use nssa_core::account::Nonce;
use rand::{RngCore, rngs::OsRng};
use serde::Serialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{
    HOME_DIR_ENV_VAR,
    config::{
        BasicAuth, InitialAccountData, InitialAccountDataPrivate, InitialAccountDataPublic,
        PersistentAccountDataPrivate, PersistentAccountDataPublic, PersistentStorage, WalletConfig,
    },
};

/// Get home dir for wallet. Env var `NSSA_WALLET_HOME_DIR` must be set before execution to succeed.
pub fn get_home_nssa_var() -> Result<PathBuf> {
    Ok(PathBuf::from_str(&std::env::var(HOME_DIR_ENV_VAR)?)?)
}

/// Get home dir for wallet. Env var `HOME` must be set before execution to succeed.
pub fn get_home_default_path() -> Result<PathBuf> {
    std::env::home_dir()
        .map(|path| path.join(".nssa").join("wallet"))
        .ok_or(anyhow::anyhow!("Failed to get HOME"))
}

/// Get home dir for wallet.
pub fn get_home() -> Result<PathBuf> {
    if let Ok(home) = get_home_nssa_var() {
        Ok(home)
    } else {
        get_home_default_path()
    }
}

/// Fetch config from default home
pub async fn fetch_config() -> Result<WalletConfig> {
    let config_home = get_home()?;
    let mut config_needs_setup = false;

    let config = match tokio::fs::OpenOptions::new()
        .read(true)
        .open(config_home.join("wallet_config.json"))
        .await
    {
        Ok(mut file) => {
            let mut config_contents = vec![];
            file.read_to_end(&mut config_contents).await?;

            serde_json::from_slice(&config_contents)?
        }
        Err(err) => match err.kind() {
            std::io::ErrorKind::NotFound => {
                config_needs_setup = true;

                println!("Config not found, setting up default config");

                WalletConfig::default()
            }
            _ => anyhow::bail!("IO error {err:#?}"),
        },
    };

    if config_needs_setup {
        tokio::fs::create_dir_all(&config_home).await?;

        println!("Created configs dir at path {config_home:#?}");

        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(config_home.join("wallet_config.json"))
            .await?;

        let default_config_serialized =
            serde_json::to_vec_pretty(&WalletConfig::default()).unwrap();

        file.write_all(&default_config_serialized).await?;

        println!("Configs setted up");
    }

    Ok(config)
}

/// Parse CLI auth string and merge with config auth, prioritizing CLI
pub fn merge_auth_config(
    mut config: WalletConfig,
    cli_auth: Option<String>,
) -> Result<WalletConfig> {
    if let Some(auth_str) = cli_auth {
        let cli_auth_config: BasicAuth = auth_str.parse()?;

        if config.basic_auth.is_some() {
            println!("Warning: CLI auth argument takes precedence over config basic-auth");
        }

        config.basic_auth = Some(cli_auth_config);
    }
    Ok(config)
}

/// Fetch data stored at home
///
/// File must be created through setup beforehand.
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
            std::io::ErrorKind::NotFound => {
                anyhow::bail!("Not found, please setup roots from config command beforehand");
            }
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

    for (account_id, key) in &user_data.public_key_tree.account_id_map {
        if let Some(data) = user_data.public_key_tree.key_map.get(key) {
            vec_for_storage.push(
                PersistentAccountDataPublic {
                    account_id: *account_id,
                    chain_index: key.clone(),
                    data: data.clone(),
                }
                .into(),
            );
        }
    }

    for (account_id, key) in &user_data.private_key_tree.account_id_map {
        if let Some(data) = user_data.private_key_tree.key_map.get(key) {
            vec_for_storage.push(
                PersistentAccountDataPrivate {
                    account_id: *account_id,
                    chain_index: key.clone(),
                    data: data.clone(),
                }
                .into(),
            );
        }
    }

    for (account_id, key) in &user_data.default_pub_account_signing_keys {
        vec_for_storage.push(
            InitialAccountData::Public(InitialAccountDataPublic {
                account_id: account_id.to_string(),
                pub_sign_key: key.clone(),
            })
            .into(),
        )
    }

    for (account_id, (key_chain, account)) in &user_data.default_user_private_accounts {
        vec_for_storage.push(
            InitialAccountData::Private(InitialAccountDataPrivate {
                account_id: account_id.to_string(),
                account: account.clone(),
                key_chain: key_chain.clone(),
            })
            .into(),
        )
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
pub enum AccountPrivacyKind {
    Public,
    Private,
}

pub(crate) fn parse_addr_with_privacy_prefix(
    account_base58: &str,
) -> Result<(String, AccountPrivacyKind)> {
    if account_base58.starts_with("Public/") {
        Ok((
            account_base58.strip_prefix("Public/").unwrap().to_string(),
            AccountPrivacyKind::Public,
        ))
    } else if account_base58.starts_with("Private/") {
        Ok((
            account_base58.strip_prefix("Private/").unwrap().to_string(),
            AccountPrivacyKind::Private,
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

        assert_eq!(addr_kind, AccountPrivacyKind::Public);

        let addr_base58 = "Private/BLgCRDXYdQPMMWVHYRFGQZbgeHx9frkipa8GtpG2Syqy";
        let (_, addr_kind) = parse_addr_with_privacy_prefix(addr_base58).unwrap();

        assert_eq!(addr_kind, AccountPrivacyKind::Private);

        let addr_base58 = "asdsada/BLgCRDXYdQPMMWVHYRFGQZbgeHx9frkipa8GtpG2Syqy";
        assert!(parse_addr_with_privacy_prefix(addr_base58).is_err());
    }
}

use std::{fs::File, io::BufReader, path::PathBuf, str::FromStr};

use anyhow::Result;
use key_protocol::key_protocol_core::NSSAUserData;
use nssa::Address;

use crate::{
    HOME_DIR_ENV_VAR,
    config::{PersistentAccountData, WalletConfig},
};

///Get home dir for wallet. Env var `NSSA_WALLET_HOME_DIR` must be set before execution to succeed.
pub fn get_home() -> Result<PathBuf> {
    Ok(PathBuf::from_str(&std::env::var(HOME_DIR_ENV_VAR)?)?)
}

///Fetch config from `NSSA_WALLET_HOME_DIR`
pub fn fetch_config() -> Result<WalletConfig> {
    let config_home = get_home()?;
    let file = File::open(config_home.join("wallet_config.json"))?;
    let reader = BufReader::new(file);

    Ok(serde_json::from_reader(reader)?)
}

//ToDo: Replace with structures conversion in future
pub fn produce_account_addr_from_hex(hex_str: String) -> Result<Address> {
    Ok(hex_str.parse()?)
}

///Fetch list of accounts stored at `NSSA_WALLET_HOME_DIR/curr_accounts.json`
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

///Produces a list of accounts for storage
pub fn produce_data_for_storage(user_data: &NSSAUserData) -> Vec<PersistentAccountData> {
    let mut vec_for_storage = vec![];

    for (addr, key) in &user_data.key_holder.pub_account_signing_keys {
        vec_for_storage.push(PersistentAccountData {
            address: *addr,
            pub_sign_key: key.clone(),
        });
    }

    vec_for_storage
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

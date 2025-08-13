use std::{fs::File, io::BufReader, path::PathBuf, str::FromStr};

use anyhow::Result;
use nssa::{address::HexString, Address};

use crate::{config::WalletConfig, HOME_DIR_ENV_VAR};

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
    Ok(HexString::try_from(hex_str.as_str())?.into())
}

///Fetch list of accounts stored at `NSSA_WALLET_HOME_DIR/curr_accounts.json`
///
/// If file not present, it is considered as empty list of persistent accounts
pub fn fetch_persistent_accounts() -> Result<Vec<Account>> {
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

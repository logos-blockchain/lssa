pub mod native_token_transfer;
pub mod pinata;
pub mod token;

use anyhow::Result;
use clap::Args;
use nssa::AccountId;

use crate::helperfunctions::{AccountPrivacyKind, parse_addr_with_privacy_prefix};

#[derive(Debug, Args)]
struct ArgsSenderOwned {
    /// from - valid 32 byte base58 string with privacy prefix
    #[arg(long)]
    from: String,
}

impl ArgsSenderOwned {
    pub fn parse_acc_and_privacy(self) -> Result<(AccountId, AccountPrivacyKind)> {
        let (acc_id_raw, privacy) = parse_addr_with_privacy_prefix(&self.from)?;
        Ok((acc_id_raw.parse()?, privacy))
    }
}

#[derive(Debug, Args)]
struct ArgsReceiverVariableOwnership {
    /// to - valid 32 byte base58 string with privacy prefix
    #[arg(long)]
    to: Option<String>,
    /// to_npk - valid 32 byte hex string
    #[arg(long)]
    to_npk: Option<String>,
    /// to_ipk - valid 33 byte hex string
    #[arg(long)]
    to_ipk: Option<String>,
}

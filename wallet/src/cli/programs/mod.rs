pub mod native_token_transfer;
pub mod pinata;
pub mod token;

use anyhow::Result;
use clap::Args;

use crate::{
    PrivacyPreservingAccount,
    helperfunctions::{AccountPrivacyKind, parse_addr_with_privacy_prefix},
};

trait ParsePrivacyPreservingAccount {
    fn parse(&self) -> Result<PrivacyPreservingAccount>;
}

#[derive(Debug, Args, Clone)]
pub struct ArgsSenderOwned {
    /// from - valid 32 byte base58 string with privacy prefix
    #[arg(long)]
    pub from: String,
}

impl ParsePrivacyPreservingAccount for ArgsSenderOwned {
    fn parse(&self) -> Result<PrivacyPreservingAccount> {
        let (account_id, privacy) = parse_addr_with_privacy_prefix(&self.from)?;

        match privacy {
            AccountPrivacyKind::Public => Ok(PrivacyPreservingAccount::Public(account_id.parse()?)),
            AccountPrivacyKind::Private => {
                Ok(PrivacyPreservingAccount::PrivateOwned(account_id.parse()?))
            }
        }
    }
}

#[derive(Debug, Args, Clone)]
pub struct ArgsReceiverMaybeUnowned {
    /// to - valid 32 byte base58 string with privacy prefix
    #[arg(long)]
    pub to: Option<String>,
    /// to_npk - valid 32 byte hex string
    #[arg(long)]
    pub to_npk: Option<String>,
    /// to_ipk - valid 33 byte hex string
    #[arg(long)]
    pub to_ipk: Option<String>,
}

impl ParsePrivacyPreservingAccount for ArgsReceiverMaybeUnowned {
    fn parse(&self) -> Result<PrivacyPreservingAccount> {
        match (&self.to, &self.to_npk, &self.to_ipk) {
            (None, None, None) => {
                anyhow::bail!("Provide either account account_id of receiver or their public keys");
            }
            (Some(_), Some(_), Some(_)) => {
                anyhow::bail!(
                    "Provide only one variant: either account account_id of receiver or their public keys"
                );
            }
            (_, Some(_), None) | (_, None, Some(_)) => {
                anyhow::bail!("List of public keys is uncomplete");
            }
            (Some(to), None, None) => ArgsSenderOwned { from: to.clone() }.parse(),
            (None, Some(to_npk), Some(to_ipk)) => {
                let to_npk_res = hex::decode(to_npk)?;
                let mut to_npk = [0; 32];
                to_npk.copy_from_slice(&to_npk_res);
                let to_npk = nssa_core::NullifierPublicKey(to_npk);

                let to_ipk_res = hex::decode(to_ipk)?;
                let mut to_ipk = [0u8; 33];
                to_ipk.copy_from_slice(&to_ipk_res);
                let to_ipk =
                    nssa_core::encryption::shared_key_derivation::Secp256k1Point(to_ipk.to_vec());

                Ok(PrivacyPreservingAccount::PrivateForeign {
                    npk: to_npk,
                    ipk: to_ipk,
                })
            }
        }
    }
}

#[derive(Debug, Args, Clone)]
pub struct ArgsDefinitionOwned {
    /// definition_account_id - valid 32 byte base58 string with privacy prefix
    #[arg(long)]
    pub definition_account_id: String,
}

impl ParsePrivacyPreservingAccount for ArgsDefinitionOwned {
    fn parse(&self) -> Result<PrivacyPreservingAccount> {
        let (account_id, privacy) = parse_addr_with_privacy_prefix(&self.definition_account_id)?;

        match privacy {
            AccountPrivacyKind::Public => Ok(PrivacyPreservingAccount::Public(account_id.parse()?)),
            AccountPrivacyKind::Private => {
                Ok(PrivacyPreservingAccount::PrivateOwned(account_id.parse()?))
            }
        }
    }
}

#[derive(Debug, Args, Clone)]
pub struct ArgsSupplyOwned {
    /// supply_account_id - valid 32 byte base58 string with privacy prefix
    #[arg(long)]
    pub supply_account_id: String,
}

impl ParsePrivacyPreservingAccount for ArgsSupplyOwned {
    fn parse(&self) -> Result<PrivacyPreservingAccount> {
        let (account_id, privacy) = parse_addr_with_privacy_prefix(&self.supply_account_id)?;

        match privacy {
            AccountPrivacyKind::Public => Ok(PrivacyPreservingAccount::Public(account_id.parse()?)),
            AccountPrivacyKind::Private => {
                Ok(PrivacyPreservingAccount::PrivateOwned(account_id.parse()?))
            }
        }
    }
}

#[derive(Debug, Args, Clone)]
pub struct ArgsHolderOwned {
    /// holder_account_id - valid 32 byte base58 string with privacy prefix
    #[arg(long)]
    pub holder_account_id: String,
}

impl ParsePrivacyPreservingAccount for ArgsHolderOwned {
    fn parse(&self) -> Result<PrivacyPreservingAccount> {
        let (account_id, privacy) = parse_addr_with_privacy_prefix(&self.holder_account_id)?;

        match privacy {
            AccountPrivacyKind::Public => Ok(PrivacyPreservingAccount::Public(account_id.parse()?)),
            AccountPrivacyKind::Private => {
                Ok(PrivacyPreservingAccount::PrivateOwned(account_id.parse()?))
            }
        }
    }
}

#[derive(Debug, Args, Clone)]
pub struct ArgsHolderMaybeUnowned {
    /// holder - valid 32 byte base58 string with privacy prefix
    #[arg(long)]
    pub holder: Option<String>,
    /// holder_npk - valid 32 byte hex string
    #[arg(long)]
    pub holder_npk: Option<String>,
    /// holder_ipk - valid 33 byte hex string
    #[arg(long)]
    pub holder_ipk: Option<String>,
}

impl ParsePrivacyPreservingAccount for ArgsHolderMaybeUnowned {
    fn parse(&self) -> Result<PrivacyPreservingAccount> {
        match (&self.holder, &self.holder_npk, &self.holder_ipk) {
            (None, None, None) => {
                anyhow::bail!("Provide either account account_id of receiver or their public keys");
            }
            (Some(_), Some(_), Some(_)) => {
                anyhow::bail!(
                    "Provide only one variant: either account account_id of receiver or their public keys"
                );
            }
            (_, Some(_), None) | (_, None, Some(_)) => {
                anyhow::bail!("List of public keys is uncomplete");
            }
            (Some(holder), None, None) => ArgsSenderOwned {
                from: holder.clone(),
            }
            .parse(),
            (None, Some(holder_npk), Some(holder_ipk)) => {
                let holder_npk_res = hex::decode(holder_npk)?;
                let mut holder_npk = [0; 32];
                holder_npk.copy_from_slice(&holder_npk_res);
                let holder_npk = nssa_core::NullifierPublicKey(holder_npk);

                let holder_ipk_res = hex::decode(holder_ipk)?;
                let mut holder_ipk = [0u8; 33];
                holder_ipk.copy_from_slice(&holder_ipk_res);
                let holder_ipk = nssa_core::encryption::shared_key_derivation::Secp256k1Point(
                    holder_ipk.to_vec(),
                );

                Ok(PrivacyPreservingAccount::PrivateForeign {
                    npk: holder_npk,
                    ipk: holder_ipk,
                })
            }
        }
    }
}

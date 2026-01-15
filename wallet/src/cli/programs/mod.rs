pub mod amm;
pub mod native_token_transfer;
pub mod pinata;
pub mod token;

use anyhow::Result;
use clap::Args;
use paste::paste;

use crate::{
    PrivacyPreservingAccount,
    helperfunctions::{AccountPrivacyKind, parse_addr_with_privacy_prefix},
};

trait ParsePrivacyPreservingAccount {
    fn parse(&self) -> Result<PrivacyPreservingAccount>;
}

#[macro_export]
macro_rules! owned_account_name {
    ($structname: ident, $field: ident) => {
        #[derive(Debug, Args, Clone)]
        pub struct $structname {
            /// $field - valid 32 byte base58 string with privacy prefix
            #[arg(long)]
            pub $field: String,
        }

        impl ParsePrivacyPreservingAccount for $structname {
            fn parse(&self) -> Result<PrivacyPreservingAccount> {
                let (account_id, privacy) = parse_addr_with_privacy_prefix(&self.$field)?;

                match privacy {
                    AccountPrivacyKind::Public => {
                        Ok(PrivacyPreservingAccount::Public(account_id.parse()?))
                    }
                    AccountPrivacyKind::Private => {
                        Ok(PrivacyPreservingAccount::PrivateOwned(account_id.parse()?))
                    }
                }
            }
        }
    };
}

owned_account_name!(ArgsSenderOwned, from);

#[macro_export]
macro_rules! maybe_unowned_account_name {
    ($structname: ident, $field: ident) => {
        paste! {
        #[derive(Debug, Args, Clone)]
        pub struct $structname {
            /// $field - valid 32 byte base58 string with privacy prefix
            #[arg(long)]
            pub $field: Option<String>,
            /// [<$field _npk>] - valid 32 byte hex string
            #[arg(long)]
            pub [<$field _npk>]: Option<String>,
            /// [<$field _ipk>] - valid 33 byte hex string
            #[arg(long)]
            pub [<$field _ipk>]: Option<String>,
        }

        impl ParsePrivacyPreservingAccount for $structname {
            fn parse(&self) -> Result<PrivacyPreservingAccount> {
                match (&self.$field, &self.[<$field _npk>], &self.[<$field _ipk>]) {
                    (None, None, None) => {
                        anyhow::bail!("Provide either account account_id or their public keys");
                    }
                    (Some(_), Some(_), Some(_)) => {
                        anyhow::bail!(
                            "Provide only one variant: either account account_id or their public keys"
                        );
                    }
                    (_, Some(_), None) | (_, None, Some(_)) => {
                        anyhow::bail!("List of public keys is incomplete");
                    }
                    (Some($field), None, None) => ArgsSenderOwned {
                        from: $field.clone(),
                    }
                    .parse(),
                    (None, Some(npk), Some(ipk)) => {
                        let npk_res = hex::decode(npk)?;
                        let mut npk = [0; 32];
                        npk.copy_from_slice(&npk_res);
                        let npk = nssa_core::NullifierPublicKey(npk);

                        let ipk_res = hex::decode(ipk)?;
                        let mut ipk = [0u8; 33];
                        ipk.copy_from_slice(&ipk_res);
                        let ipk = nssa_core::encryption::shared_key_derivation::Secp256k1Point(
                            ipk.to_vec(),
                        );

                        Ok(PrivacyPreservingAccount::PrivateForeign {
                            npk,
                            ipk,
                        })
                    }
                }
            }
        }
        }
    };
}

maybe_unowned_account_name!(ArgsReceiverMaybeUnowned, to);

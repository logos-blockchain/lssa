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

macro_rules! owned_account_name {
    ($classname: ident, $field: ident) => {
        #[derive(Debug, Args, Clone)]
        pub struct $classname {
            /// $field - valid 32 byte base58 string with privacy prefix
            #[arg(long)]
            pub $field: String,
        }

        impl ParsePrivacyPreservingAccount for $classname {
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
owned_account_name!(ArgsDefinitionOwned, definition_account_id);
owned_account_name!(ArgsSupplyOwned, supply_account_id);
owned_account_name!(ArgsHolderOwned, holder_account_id);

macro_rules! maybe_unowned_account_name {
    ($classname: ident, $field: ident) => {
        paste! {
        #[derive(Debug, Args, Clone)]
        pub struct $classname {
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

        impl ParsePrivacyPreservingAccount for $classname {
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
                        anyhow::bail!("List of public keys is uncomplete");
                    }
                    (Some($field), None, None) => ArgsSenderOwned {
                        from: $field.clone(),
                    }
                    .parse(),
                    (None, Some([<$field _npk>]), Some([<$field _ipk>])) => {
                        let [<$field _npk_res>] = hex::decode([<$field _npk>])?;
                        let mut [<$field _npk>] = [0; 32];
                        [<$field _npk>].copy_from_slice(&[<$field _npk_res>]);
                        let [<$field _npk>] = nssa_core::NullifierPublicKey([<$field _npk>]);

                        let [<$field _ipk_res>] = hex::decode([<$field _ipk>])?;
                        let mut [<$field _ipk>] = [0u8; 33];
                        [<$field _ipk>].copy_from_slice(&[<$field _ipk_res>]);
                        let [<$field _ipk>] = nssa_core::encryption::shared_key_derivation::Secp256k1Point(
                            [<$field _ipk>].to_vec(),
                        );

                        Ok(PrivacyPreservingAccount::PrivateForeign {
                            npk: [<$field _npk>],
                            ipk: [<$field _ipk>],
                        })
                    }
                }
            }
        }
        }
    };
}

maybe_unowned_account_name!(ArgsReceiverMaybeUnowned, to);
maybe_unowned_account_name!(ArgsHolderMaybeUnowned, holder);

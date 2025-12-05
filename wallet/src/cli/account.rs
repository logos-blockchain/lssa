use anyhow::Result;
use base58::ToBase58;
use clap::Subcommand;
use itertools::Itertools as _;
use key_protocol::key_management::key_tree::chain_index::ChainIndex;
use nssa::{Account, AccountId, program::Program};
use serde::Serialize;

use crate::{
    WalletCore,
    cli::{SubcommandReturnValue, WalletSubcommand},
    helperfunctions::{AccountPrivacyKind, HumanReadableAccount, parse_addr_with_privacy_prefix},
};

const TOKEN_DEFINITION_TYPE: u8 = 0;
const TOKEN_DEFINITION_DATA_SIZE: usize = 23;

const TOKEN_HOLDING_TYPE: u8 = 1;
const TOKEN_HOLDING_DATA_SIZE: usize = 49;

struct TokenDefinition {
    #[allow(unused)]
    account_type: u8,
    name: [u8; 6],
    total_supply: u128,
}

struct TokenHolding {
    #[allow(unused)]
    account_type: u8,
    definition_id: AccountId,
    balance: u128,
}

impl TokenDefinition {
    fn parse(data: &[u8]) -> Option<Self> {
        if data.len() != TOKEN_DEFINITION_DATA_SIZE || data[0] != TOKEN_DEFINITION_TYPE {
            None
        } else {
            let account_type = data[0];
            let name = data[1..7].try_into().unwrap();
            let total_supply = u128::from_le_bytes(data[7..].try_into().unwrap());

            Some(Self {
                account_type,
                name,
                total_supply,
            })
        }
    }
}

impl TokenHolding {
    fn parse(data: &[u8]) -> Option<Self> {
        if data.len() != TOKEN_HOLDING_DATA_SIZE || data[0] != TOKEN_HOLDING_TYPE {
            None
        } else {
            let account_type = data[0];
            let definition_id = AccountId::new(data[1..33].try_into().unwrap());
            let balance = u128::from_le_bytes(data[33..].try_into().unwrap());
            Some(Self {
                definition_id,
                balance,
                account_type,
            })
        }
    }
}

/// Represents generic chain CLI subcommand
#[derive(Subcommand, Debug, Clone)]
pub enum AccountSubcommand {
    /// Get account data
    Get {
        /// Flag to get raw account data
        #[arg(short, long)]
        raw: bool,
        /// Valid 32 byte base58 string with privacy prefix
        #[arg(short, long)]
        account_id: String,
    },
    /// Produce new public or private account
    #[command(subcommand)]
    New(NewSubcommand),
    /// Sync private accounts
    SyncPrivate {},
    /// List all accounts owned by the wallet
    #[command(visible_alias = "ls")]
    List {},
}

/// Represents generic register CLI subcommand
#[derive(Subcommand, Debug, Clone)]
pub enum NewSubcommand {
    /// Register new public account
    Public {
        #[arg(long)]
        /// Chain index of a parent node
        cci: ChainIndex,
    },
    /// Register new private account
    Private {
        #[arg(long)]
        /// Chain index of a parent node
        cci: ChainIndex,
    },
}

impl WalletSubcommand for NewSubcommand {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            NewSubcommand::Public { cci } => {
                let account_id = wallet_core.create_new_account_public(cci).await;

                println!("Generated new account with account_id Public/{account_id}");

                let path = wallet_core.store_persistent_data().await?;

                println!("Stored persistent accounts at {path:#?}");

                Ok(SubcommandReturnValue::RegisterAccount { account_id })
            }
            NewSubcommand::Private { cci } => {
                let account_id = wallet_core.create_new_account_private(cci);

                let (key, _) = wallet_core
                    .storage
                    .user_data
                    .get_private_account(&account_id)
                    .unwrap();

                println!(
                    "Generated new account with account_id Private/{}",
                    account_id.to_bytes().to_base58()
                );
                println!("With npk {}", hex::encode(key.nullifer_public_key.0));
                println!(
                    "With ipk {}",
                    hex::encode(key.incoming_viewing_public_key.to_bytes())
                );

                let path = wallet_core.store_persistent_data().await?;

                println!("Stored persistent accounts at {path:#?}");

                Ok(SubcommandReturnValue::RegisterAccount { account_id })
            }
        }
    }
}

#[derive(Debug, Serialize)]
pub struct AuthenticatedTransferAccountView {
    pub balance: u128,
}

impl From<nssa::Account> for AuthenticatedTransferAccountView {
    fn from(value: nssa::Account) -> Self {
        Self {
            balance: value.balance,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct TokedDefinitionAccountView {
    pub account_type: String,
    pub name: String,
    pub total_supply: u128,
}

impl From<TokenDefinition> for TokedDefinitionAccountView {
    fn from(value: TokenDefinition) -> Self {
        Self {
            account_type: "Token definition".to_string(),
            name: {
                // Assuming, that name does not have UTF-8 NULL and all zeroes are padding.
                let name_trimmed: Vec<_> =
                    value.name.into_iter().take_while(|ch| *ch != 0).collect();
                String::from_utf8(name_trimmed).unwrap_or(hex::encode(value.name))
            },
            total_supply: value.total_supply,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct TokedHoldingAccountView {
    pub account_type: String,
    pub definition_id: String,
    pub balance: u128,
}

impl From<TokenHolding> for TokedHoldingAccountView {
    fn from(value: TokenHolding) -> Self {
        Self {
            account_type: "Token holding".to_string(),
            definition_id: value.definition_id.to_string(),
            balance: value.balance,
        }
    }
}

impl WalletSubcommand for AccountSubcommand {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            AccountSubcommand::Get { raw, account_id } => {
                let (account_id, addr_kind) = parse_addr_with_privacy_prefix(&account_id)?;

                let account_id = account_id.parse()?;

                let account = match addr_kind {
                    AccountPrivacyKind::Public => {
                        wallet_core.get_account_public(account_id).await?
                    }
                    AccountPrivacyKind::Private => wallet_core
                        .get_account_private(&account_id)
                        .ok_or(anyhow::anyhow!("Private account not found in storage"))?,
                };

                if account == Account::default() {
                    println!("Account is Uninitialized");

                    return Ok(SubcommandReturnValue::Empty);
                }

                if raw {
                    let account_hr: HumanReadableAccount = account.clone().into();
                    println!("{}", serde_json::to_string(&account_hr).unwrap());

                    return Ok(SubcommandReturnValue::Empty);
                }

                let auth_tr_prog_id = Program::authenticated_transfer_program().id();
                let token_prog_id = Program::token().id();

                let acc_view = match &account.program_owner {
                    _ if account.program_owner == auth_tr_prog_id => {
                        let acc_view: AuthenticatedTransferAccountView = account.into();

                        println!("Account owned by authenticated transfer program");

                        serde_json::to_string(&acc_view)?
                    }
                    _ if account.program_owner == token_prog_id => {
                        if let Some(token_def) = TokenDefinition::parse(&account.data) {
                            let acc_view: TokedDefinitionAccountView = token_def.into();

                            println!("Definition account owned by token program");

                            serde_json::to_string(&acc_view)?
                        } else if let Some(token_hold) = TokenHolding::parse(&account.data) {
                            let acc_view: TokedHoldingAccountView = token_hold.into();

                            println!("Holding account owned by token program");

                            serde_json::to_string(&acc_view)?
                        } else {
                            anyhow::bail!(
                                "Invalid data for account {account_id:#?} with token program"
                            );
                        }
                    }
                    _ => {
                        let account_hr: HumanReadableAccount = account.clone().into();
                        serde_json::to_string(&account_hr).unwrap()
                    }
                };

                println!("{}", acc_view);

                Ok(SubcommandReturnValue::Empty)
            }
            AccountSubcommand::New(new_subcommand) => {
                new_subcommand.handle_subcommand(wallet_core).await
            }
            AccountSubcommand::SyncPrivate {} => {
                let curr_last_block = wallet_core
                    .sequencer_client
                    .get_last_block()
                    .await?
                    .last_block;

                if wallet_core
                    .storage
                    .user_data
                    .private_key_tree
                    .account_id_map
                    .is_empty()
                {
                    wallet_core.last_synced_block = curr_last_block;

                    let path = wallet_core.store_persistent_data().await?;

                    println!("Stored persistent data at {path:#?}");
                } else {
                    wallet_core.sync_to_block(curr_last_block).await?;
                }

                Ok(SubcommandReturnValue::SyncedToBlock(curr_last_block))
            }
            AccountSubcommand::List {} => {
                let user_data = &wallet_core.storage.user_data;
                let accounts = user_data
                    .default_pub_account_signing_keys
                    .keys()
                    .map(|id| format!("Preconfigured Public/{id}"))
                    .chain(
                        user_data
                            .default_user_private_accounts
                            .keys()
                            .map(|id| format!("Preconfigured Private/{id}")),
                    )
                    .chain(
                        user_data
                            .public_key_tree
                            .account_id_map
                            .iter()
                            .map(|(id, chain_index)| format!("{chain_index} Public/{id}")),
                    )
                    .chain(
                        user_data
                            .private_key_tree
                            .account_id_map
                            .iter()
                            .map(|(id, chain_index)| format!("{chain_index} Private/{id}")),
                    )
                    .format(",\n");

                println!("{accounts}");
                Ok(SubcommandReturnValue::Empty)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cli::account::{TokedDefinitionAccountView, TokenDefinition};

    #[test]
    fn test_invalid_utf_8_name_of_token() {
        let token_def = TokenDefinition {
            account_type: 1,
            name: [137, 12, 14, 3, 5, 4],
            total_supply: 100,
        };

        let token_def_view: TokedDefinitionAccountView = token_def.into();

        assert_eq!(token_def_view.name, "890c0e030504");
    }

    #[test]
    fn test_valid_utf_8_name_of_token_all_bytes() {
        let token_def = TokenDefinition {
            account_type: 1,
            name: [240, 159, 146, 150, 66, 66],
            total_supply: 100,
        };

        let token_def_view: TokedDefinitionAccountView = token_def.into();

        assert_eq!(token_def_view.name, "💖BB");
    }

    #[test]
    fn test_valid_utf_8_name_of_token_less_bytes() {
        let token_def = TokenDefinition {
            account_type: 1,
            name: [78, 65, 77, 69, 0, 0],
            total_supply: 100,
        };

        let token_def_view: TokedDefinitionAccountView = token_def.into();

        assert_eq!(token_def_view.name, "NAME");
    }
}

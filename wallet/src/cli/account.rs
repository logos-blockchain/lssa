use anyhow::Result;
use base58::ToBase58;
use clap::Subcommand;
use itertools::Itertools as _;
use key_protocol::key_management::key_tree::chain_index::ChainIndex;
use nssa::{Account, program::Program};
use serde::Serialize;

use crate::{
    TokenDefinition, TokenHolding, WalletCore,
    cli::{SubcommandReturnValue, WalletSubcommand},
    helperfunctions::{AccountPrivacyKind, HumanReadableAccount, parse_addr_with_privacy_prefix},
};

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
    List {
        /// Show detailed account information (like `account get`)
        #[arg(short, long)]
        long: bool,
    },
}

/// Represents generic register CLI subcommand
#[derive(Subcommand, Debug, Clone)]
pub enum NewSubcommand {
    /// Register new public account
    Public {
        #[arg(long)]
        /// Chain index of a parent node
        cci: Option<ChainIndex>,
    },
    /// Register new private account
    Private {
        #[arg(long)]
        /// Chain index of a parent node
        cci: Option<ChainIndex>,
    },
}

impl WalletSubcommand for NewSubcommand {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            NewSubcommand::Public { cci } => {
                let (account_id, chain_index) = wallet_core.create_new_account_public(cci);

                println!(
                    "Generated new account with account_id Public/{account_id} at path {chain_index}"
                );

                wallet_core.store_persistent_data().await?;

                Ok(SubcommandReturnValue::RegisterAccount { account_id })
            }
            NewSubcommand::Private { cci } => {
                let (account_id, chain_index) = wallet_core.create_new_account_private(cci);

                let (key, _) = wallet_core
                    .storage
                    .user_data
                    .get_private_account(&account_id)
                    .unwrap();

                println!(
                    "Generated new account with account_id Private/{} at path {chain_index}",
                    account_id.to_bytes().to_base58()
                );
                println!("With npk {}", hex::encode(key.nullifer_public_key.0));
                println!(
                    "With ipk {}",
                    hex::encode(key.incoming_viewing_public_key.to_bytes())
                );

                wallet_core.store_persistent_data().await?;

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

/// Formats account details for display, returning (description, json_view)
fn format_account_details(account: &Account) -> (String, String) {
    let auth_tr_prog_id = Program::authenticated_transfer_program().id();
    let token_prog_id = Program::token().id();

    match &account.program_owner {
        _ if account.program_owner == auth_tr_prog_id => {
            let acc_view: AuthenticatedTransferAccountView = account.clone().into();
            (
                "Account owned by authenticated transfer program".to_string(),
                serde_json::to_string(&acc_view).unwrap(),
            )
        }
        _ if account.program_owner == token_prog_id => {
            if let Some(token_def) = TokenDefinition::parse(&account.data) {
                let acc_view: TokedDefinitionAccountView = token_def.into();
                (
                    "Definition account owned by token program".to_string(),
                    serde_json::to_string(&acc_view).unwrap(),
                )
            } else if let Some(token_hold) = TokenHolding::parse(&account.data) {
                let acc_view: TokedHoldingAccountView = token_hold.into();
                (
                    "Holding account owned by token program".to_string(),
                    serde_json::to_string(&acc_view).unwrap(),
                )
            } else {
                let account_hr: HumanReadableAccount = account.clone().into();
                (
                    "Unknown token program account".to_string(),
                    serde_json::to_string(&account_hr).unwrap(),
                )
            }
        }
        _ => {
            let account_hr: HumanReadableAccount = account.clone().into();
            (
                "Account".to_string(),
                serde_json::to_string(&account_hr).unwrap(),
            )
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

                let (description, json_view) = format_account_details(&account);
                println!("{description}");
                println!("{json_view}");

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

                    wallet_core.store_persistent_data().await?;
                } else {
                    wallet_core.sync_to_block(curr_last_block).await?;
                }

                Ok(SubcommandReturnValue::SyncedToBlock(curr_last_block))
            }
            AccountSubcommand::List { long } => {
                let user_data = &wallet_core.storage.user_data;

                if !long {
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
                    return Ok(SubcommandReturnValue::Empty);
                }

                // Detailed listing with --long flag
                // Preconfigured public accounts
                for id in user_data.default_pub_account_signing_keys.keys() {
                    println!("Preconfigured Public/{id}");
                    match wallet_core.get_account_public(*id).await {
                        Ok(account) if account != Account::default() => {
                            let (description, json_view) = format_account_details(&account);
                            println!("  {description}");
                            println!("  {json_view}");
                        }
                        Ok(_) => println!("  Uninitialized"),
                        Err(e) => println!("  Error fetching account: {e}"),
                    }
                }

                // Preconfigured private accounts
                for id in user_data.default_user_private_accounts.keys() {
                    println!("Preconfigured Private/{id}");
                    match wallet_core.get_account_private(id) {
                        Some(account) if account != Account::default() => {
                            let (description, json_view) = format_account_details(&account);
                            println!("  {description}");
                            println!("  {json_view}");
                        }
                        Some(_) => println!("  Uninitialized"),
                        None => println!("  Not found in local storage"),
                    }
                }

                // Public key tree accounts
                for (id, chain_index) in user_data.public_key_tree.account_id_map.iter() {
                    println!("{chain_index} Public/{id}");
                    match wallet_core.get_account_public(*id).await {
                        Ok(account) if account != Account::default() => {
                            let (description, json_view) = format_account_details(&account);
                            println!("  {description}");
                            println!("  {json_view}");
                        }
                        Ok(_) => println!("  Uninitialized"),
                        Err(e) => println!("  Error fetching account: {e}"),
                    }
                }

                // Private key tree accounts
                for (id, chain_index) in user_data.private_key_tree.account_id_map.iter() {
                    println!("{chain_index} Private/{id}");
                    match wallet_core.get_account_private(id) {
                        Some(account) if account != Account::default() => {
                            let (description, json_view) = format_account_details(&account);
                            println!("  {description}");
                            println!("  {json_view}");
                        }
                        Some(_) => println!("  Uninitialized"),
                        None => println!("  Not found in local storage"),
                    }
                }

                Ok(SubcommandReturnValue::Empty)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use nssa::AccountId;

    use crate::cli::account::{TokedDefinitionAccountView, TokenDefinition};

    #[test]
    fn test_invalid_utf_8_name_of_token() {
        let token_def = TokenDefinition {
            account_type: 1,
            name: [137, 12, 14, 3, 5, 4],
            total_supply: 100,
            metadata_id: AccountId::new([0; 32]),
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
            metadata_id: AccountId::new([0; 32]),
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
            metadata_id: AccountId::new([0; 32]),
        };

        let token_def_view: TokedDefinitionAccountView = token_def.into();

        assert_eq!(token_def_view.name, "NAME");
    }
}

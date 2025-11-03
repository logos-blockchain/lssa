use anyhow::Result;
use base58::ToBase58;
use clap::Subcommand;
use nssa::{Account, Address, program::Program};
use serde::Serialize;

use crate::{
    SubcommandReturnValue, WalletCore,
    cli::WalletSubcommand,
    helperfunctions::{AddressPrivacyKind, HumanReadableAccount, parse_addr_with_privacy_prefix},
    parse_block_range,
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
    definition_id: Address,
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
            let definition_id = Address::new(data[1..33].try_into().unwrap());
            let balance = u128::from_le_bytes(data[33..].try_into().unwrap());
            Some(Self {
                definition_id,
                balance,
                account_type,
            })
        }
    }
}

///Represents generic chain CLI subcommand
#[derive(Subcommand, Debug, Clone)]
pub enum AccountSubcommand {
    ///Get account data
    Get {
        ///Flag to get raw account data
        #[arg(short, long)]
        raw: bool,
        ///Valid 32 byte base58 string with privacy prefix
        #[arg(short, long)]
        addr: String,
    },
    ///Produce new public or private account  
    #[command(subcommand)]
    New(NewSubcommand),
    ///Sync private accounts
    SyncPrivate {},
}

///Represents generic register CLI subcommand
#[derive(Subcommand, Debug, Clone)]
pub enum NewSubcommand {
    ///Register new public account
    Public {},
    ///Register new private account
    Private {},
}

impl WalletSubcommand for NewSubcommand {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            NewSubcommand::Public {} => {
                let addr = wallet_core.create_new_account_public();

                println!("Generated new account with addr Public/{addr}");

                let path = wallet_core.store_persistent_data().await?;

                println!("Stored persistent accounts at {path:#?}");

                Ok(SubcommandReturnValue::RegisterAccount { addr })
            }
            NewSubcommand::Private {} => {
                let addr = wallet_core.create_new_account_private();

                let (key, _) = wallet_core
                    .storage
                    .user_data
                    .get_private_account(&addr)
                    .unwrap();

                println!(
                    "Generated new account with addr Private/{}",
                    addr.to_bytes().to_base58()
                );
                println!("With npk {}", hex::encode(key.nullifer_public_key.0));
                println!(
                    "With ipk {}",
                    hex::encode(key.incoming_viewing_public_key.to_bytes())
                );

                let path = wallet_core.store_persistent_data().await?;

                println!("Stored persistent accounts at {path:#?}");

                Ok(SubcommandReturnValue::RegisterAccount { addr })
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
            name: hex::encode(value.name),
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
            AccountSubcommand::Get { raw, addr } => {
                let (addr, addr_kind) = parse_addr_with_privacy_prefix(&addr)?;

                let addr = addr.parse()?;

                let account = match addr_kind {
                    AddressPrivacyKind::Public => wallet_core.get_account_public(addr).await?,
                    AddressPrivacyKind::Private => wallet_core
                        .get_account_private(&addr)
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
                            anyhow::bail!("Invalid data for account {addr:#?} with token program");
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
                let last_synced_block = wallet_core.last_synced_block;
                let curr_last_block = wallet_core
                    .sequencer_client
                    .get_last_block()
                    .await?
                    .last_block;

                if !wallet_core
                    .storage
                    .user_data
                    .user_private_accounts
                    .is_empty()
                {
                    parse_block_range(
                        last_synced_block + 1,
                        curr_last_block,
                        wallet_core.sequencer_client.clone(),
                        wallet_core,
                    )
                    .await?;
                } else {
                    wallet_core.last_synced_block = curr_last_block;

                    let path = wallet_core.store_persistent_data().await?;

                    println!("Stored persistent data at {path:#?}");
                }

                Ok(SubcommandReturnValue::SyncedToBlock(curr_last_block))
            }
        }
    }
}

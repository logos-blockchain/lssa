use anyhow::Result;

use crate::{SubcommandReturnValue, WalletCore};

pub mod account;
pub mod chain;
pub mod native_token_transfer_program;
pub mod pinata_program;
pub mod token_program;

pub(crate) trait WalletSubcommand {
    async fn handle_subcommand(self, wallet_core: &mut WalletCore)
    -> Result<SubcommandReturnValue>;
}

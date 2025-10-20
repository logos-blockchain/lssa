use anyhow::Result;

use crate::{SubcommandReturnValue, WalletCore};

pub mod token_program;

pub(crate) trait WalletSubcommand {
    async fn handle_subcommand(self, wallet_core: &mut WalletCore)
    -> Result<SubcommandReturnValue>;
}

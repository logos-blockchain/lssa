use anyhow::Result;
use clap::Subcommand;

use crate::{SubcommandReturnValue, WalletCore, cli::WalletSubcommand};

///Represents generic chain CLI subcommand
#[derive(Subcommand, Debug, Clone)]
pub enum ChainSubcommand {
    GetLatestBlockId {},
    GetBlockAtId {
        #[arg(short, long)]
        id: u64,
    },
    GetTransactionAtHash {
        #[arg(short, long)]
        hash: String,
    },
}

impl WalletSubcommand for ChainSubcommand {
    async fn handle_subcommand(
        self,
        wallet_core: &mut WalletCore,
    ) -> Result<SubcommandReturnValue> {
        match self {
            ChainSubcommand::GetLatestBlockId {} => {
                let latest_block_res = wallet_core.sequencer_client.get_last_block().await?;

                println!("Last block id is {}", latest_block_res.last_block);
            }
            ChainSubcommand::GetBlockAtId { id } => {
                let block_res = wallet_core.sequencer_client.get_block(id).await?;

                println!("Last block id is {:#?}", block_res.block);
            }
            ChainSubcommand::GetTransactionAtHash { hash } => {
                let tx_res = wallet_core
                    .sequencer_client
                    .get_transaction_by_hash(hash)
                    .await?;

                println!("Last block id is {:#?}", tx_res.transaction);
            }
        }
        Ok(SubcommandReturnValue::Empty)
    }
}

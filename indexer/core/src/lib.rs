use anyhow::Result;
use bedrock_client::BedrockClient;
// ToDo: Remove after testnet
use common::PINATA_BASE58;
use common::{
    block::Block,
    sequencer_client::SequencerClient,
};
use futures::StreamExt;
use log::info;
use logos_blockchain_core::mantle::{
    Op, SignedMantleTx,
    ops::channel::{ChannelId, inscribe::InscriptionOp},
};

use crate::{block_store::IndexerStore, config::IndexerConfig};

pub mod block_store;
pub mod config;
pub mod state;

#[derive(Clone)]
pub struct IndexerCore {
    pub bedrock_client: BedrockClient,
    pub sequencer_client: SequencerClient,
    pub config: IndexerConfig,
    pub store: IndexerStore,
}

impl IndexerCore {
    pub async fn new(config: IndexerConfig) -> Result<Self> {
        let sequencer_client = SequencerClient::new_with_auth(
            config.sequencer_client_config.addr.clone(),
            config.sequencer_client_config.auth.clone(),
        )?;

        let start_block = sequencer_client.get_genesis_block().await?;

        let initial_commitments: Vec<nssa_core::Commitment> = config
            .initial_commitments
            .iter()
            .map(|init_comm_data| {
                let npk = &init_comm_data.npk;

                let mut acc = init_comm_data.account.clone();

                acc.program_owner = nssa::program::Program::authenticated_transfer_program().id();

                nssa_core::Commitment::new(npk, &acc)
            })
            .collect();

        let init_accs: Vec<(nssa::AccountId, u128)> = config
            .initial_accounts
            .iter()
            .map(|acc_data| (acc_data.account_id.parse().unwrap(), acc_data.balance))
            .collect();

        let mut state = nssa::V02State::new_with_genesis_accounts(&init_accs, &initial_commitments);

        // ToDo: Remove after testnet
        state.add_pinata_program(PINATA_BASE58.parse().unwrap());

        let home = config.home.clone();

        Ok(Self {
            bedrock_client: BedrockClient::new(
                config.bedrock_client_config.auth.clone().map(Into::into),
                config.bedrock_client_config.addr.clone(),
            )?,
            sequencer_client,
            config,
            // ToDo: Implement restarts
            store: IndexerStore::open_db_with_genesis(&home, Some((start_block, state)))?,
        })
    }

    pub async fn subscribe_parse_block_stream(&self) -> impl futures::Stream<Item = Result<Block>> {
        async_stream::stream! {
        loop {
            let mut stream_pinned = Box::pin(self.bedrock_client.get_lib_stream().await?);

            info!("Block stream joined");

            while let Some(block_info) = stream_pinned.next().await {
                let header_id = block_info.header_id;

                info!("Observed L1 block at height {}", block_info.height);

                if let Some(l1_block) = self
                    .bedrock_client
                    .get_block_by_id(header_id, &self.config.backoff)
                    .await?
                {
                    info!("Extracted L1 block at height {}", block_info.height);

                    let l2_blocks_parsed = parse_blocks(
                        l1_block.into_transactions().into_iter(),
                        &self.config.channel_id,
                    ).collect::<Vec<_>>();

                    info!("Parsed {} L2 blocks", l2_blocks_parsed.len());

                    for l2_block in l2_blocks_parsed {
                        self.store.put_block(l2_block.clone())?;

                        yield Ok(l2_block);
                    }
                }
            }

            // Refetch stream after delay
            tokio::time::sleep(std::time::Duration::from_millis(
                self.config.resubscribe_interval_millis,
            ))
            .await;
        }
        }
    }
}

fn parse_blocks(
    block_txs: impl Iterator<Item = SignedMantleTx>,
    decoded_channel_id: &ChannelId,
) -> impl Iterator<Item = Block> {
    block_txs.flat_map(|tx| {
        tx.mantle_tx.ops.into_iter().filter_map(|op| match op {
            Op::ChannelInscribe(InscriptionOp {
                channel_id,
                inscription,
                ..
            }) if channel_id == *decoded_channel_id => {
                borsh::from_slice::<Block>(&inscription).ok()
            }
            _ => None,
        })
    })
}
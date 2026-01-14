use anyhow::Result;
use bedrock_client::{BasicAuthCredentials, BedrockClient};
use common::block::{BlockHash, HashableBlockData};
use futures::StreamExt;
use log::info;
use nomos_core::mantle::{
    Op, SignedMantleTx,
    ops::channel::{ChannelId, inscribe::InscriptionOp},
};
use tokio::sync::mpsc::Sender;
use url::Url;

use crate::{config::IndexerConfig, state::IndexerState};

pub mod config;
pub mod state;

pub struct IndexerCore {
    pub bedrock_client: BedrockClient,
    pub bedrock_url: Url,
    pub channel_sender: Sender<BlockHash>,
    pub config: IndexerConfig,
    pub state: IndexerState,
}

impl IndexerCore {
    pub fn new(
        addr: &str,
        auth: Option<BasicAuthCredentials>,
        sender: Sender<BlockHash>,
        config: IndexerConfig,
    ) -> Result<Self> {
        Ok(Self {
            bedrock_client: BedrockClient::new(auth)?,
            bedrock_url: Url::parse(addr)?,
            channel_sender: sender,
            config,
            // No state setup for now, future task.
            state: IndexerState {
                latest_seen_block: 0,
            },
        })
    }

    pub async fn subscribe_parse_block_stream(&self) -> Result<()> {
        let mut stream_pinned = Box::pin(
            self.bedrock_client
                .0
                .get_lib_stream(self.bedrock_url.clone())
                .await?,
        );

        info!("Block stream joined");

        while let Some(block_info) = stream_pinned.next().await {
            let header_id = block_info.header_id;

            info!("Observed L1 block at height {}", block_info.height);

            if let Some(l1_block) = self
                .bedrock_client
                .0
                .get_block_by_id(self.bedrock_url.clone(), header_id)
                .await?
            {
                info!("Extracted L1 block at height {} with data {l1_block:#?}", block_info.height);

                let l2_blocks_parsed = parse_blocks(
                    l1_block.into_transactions().into_iter(),
                    &self.config.channel_id,
                );

                for l2_block in l2_blocks_parsed {
                    // Sending data into sequencer, may need to be expanded.
                    self.channel_sender.send(l2_block.block_hash()).await?;
                }
            }
        }

        Ok(())
    }
}

pub fn parse_blocks(
    block_txs: impl Iterator<Item = SignedMantleTx>,
    decoded_channel_id: &ChannelId,
) -> Vec<HashableBlockData> {
    block_txs
        .flat_map(|tx| {
            tx.mantle_tx
                .ops
                .iter()
                .filter_map(|op| match op {
                    Op::ChannelInscribe(InscriptionOp {
                        channel_id,
                        inscription,
                        ..
                    }) if channel_id == decoded_channel_id => {
                        // Assuming, that it is how block will be inscribed on l1
                        borsh::from_slice::<HashableBlockData>(inscription).ok()
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

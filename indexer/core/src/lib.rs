use std::sync::Arc;

use anyhow::{Context as _, Result};
use bedrock_client::BedrockClient;
use common::block::Block;
use futures::StreamExt;
use log::{debug, info};
use logos_blockchain_core::mantle::{
    Op, SignedMantleTx,
    ops::channel::{ChannelId, inscribe::InscriptionOp},
};
use tokio::sync::RwLock;

use crate::{config::IndexerConfig, state::IndexerState};

pub mod config;
pub mod state;

#[derive(Clone)]
pub struct IndexerCore {
    bedrock_client: BedrockClient,
    config: IndexerConfig,
    state: IndexerState,
}

impl IndexerCore {
    pub fn new(config: IndexerConfig) -> Result<Self> {
        Ok(Self {
            bedrock_client: BedrockClient::new(
                config.bedrock_client_config.backoff,
                config.bedrock_client_config.addr.clone(),
                config.bedrock_client_config.auth.clone(),
            )
            .context("Failed to create Bedrock client")?,
            config,
            // No state setup for now, future task.
            state: IndexerState {
                latest_seen_block: Arc::new(RwLock::new(0)),
            },
        })
    }

    pub async fn subscribe_parse_block_stream(&self) -> impl futures::Stream<Item = Result<Block>> {
        debug!("Subscribing to Bedrock block stream");
        async_stream::stream! {
            loop {
                let mut stream_pinned = Box::pin(self.bedrock_client.get_lib_stream().await?);

                info!("Block stream joined");

                while let Some(block_info) = stream_pinned.next().await {
                    let header_id = block_info.header_id;

                    info!("Observed L1 block at height {}", block_info.height);

                    if let Some(l1_block) = self
                        .bedrock_client
                        .get_block_by_id(header_id)
                        .await?
                    {
                        info!("Extracted L1 block at height {}", block_info.height);

                        let l2_blocks_parsed = parse_blocks(
                            l1_block.into_transactions().into_iter(),
                            &self.config.channel_id,
                        ).collect::<Vec<_>>();

                        info!("Parsed {} L2 blocks", l2_blocks_parsed.len());

                        for l2_block in l2_blocks_parsed {
                            // State modification, will be updated in future
                            {
                                let mut guard = self.state.latest_seen_block.write().await;
                                if l2_block.header.block_id > *guard {
                                    *guard = l2_block.header.block_id;
                                }
                            }

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

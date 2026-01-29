use std::sync::Arc;

use anyhow::Result;
use bedrock_client::BedrockClient;
use common::{block::HashableBlockData, sequencer_client::SequencerClient};
use futures::StreamExt;
use log::info;
use logos_blockchain_core::mantle::{
    Op, SignedMantleTx,
    ops::channel::{ChannelId, inscribe::InscriptionOp},
};
use tokio::sync::RwLock;

use crate::{config::IndexerConfig, state::IndexerState};

pub mod config;
pub mod state;

pub struct IndexerCore {
    pub bedrock_client: BedrockClient,
    pub sequencer_client: SequencerClient,
    pub config: IndexerConfig,
    pub state: IndexerState,
}

impl IndexerCore {
    pub fn new(config: IndexerConfig) -> Result<Self> {
        Ok(Self {
            bedrock_client: BedrockClient::new(
                config.bedrock_client_config.auth.clone().map(Into::into),
                config.bedrock_client_config.addr.clone(),
            )?,
            sequencer_client: SequencerClient::new_with_auth(
                config.sequencer_client_config.addr.clone(),
                config.sequencer_client_config.auth.clone(),
            )?,
            config,
            // No state setup for now, future task.
            state: IndexerState {
                latest_seen_block: Arc::new(RwLock::new(0)),
            },
        })
    }

    pub async fn subscribe_parse_block_stream(&self) -> Result<()> {
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
                    );

                    for l2_block in l2_blocks_parsed {
                        // State modification, will be updated in future
                        {
                            let mut guard = self.state.latest_seen_block.write().await;
                            if l2_block.block_id > *guard {
                                *guard = l2_block.block_id;
                            }
                        }

                        // Sending data into sequencer, may need to be expanded.
                        let message = Message::L2BlockFinalized {
                            l2_block_height: l2_block.block_id,
                        };

                        let status = self.send_message_to_sequencer(message.clone()).await?;

                        info!("Sent message {message:#?} to sequencer; status {status:#?}");
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

fn parse_blocks(
    block_txs: impl Iterator<Item = SignedMantleTx>,
    decoded_channel_id: &ChannelId,
) -> impl Iterator<Item = HashableBlockData> {
    block_txs.flat_map(|tx| {
        tx.mantle_tx.ops.into_iter().filter_map(|op| match op {
            Op::ChannelInscribe(InscriptionOp {
                channel_id,
                inscription,
                ..
            }) if channel_id == *decoded_channel_id => {
                borsh::from_slice::<HashableBlockData>(&inscription).ok()
            }
            _ => None,
        })
    })
}
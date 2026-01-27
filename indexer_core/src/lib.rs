use std::sync::Arc;

use anyhow::{Context, Result};
use bedrock_client::{BasicAuthCredentials, BedrockClient};
use common::{
    block::HashableBlockData, communication::indexer::Message,
    rpc_primitives::requests::PostIndexerMessageResponse, sequencer_client::SequencerClient,
};
use futures::StreamExt;
use log::info;
use logos_blockchain_core::mantle::{
    Op, SignedMantleTx,
    ops::channel::{ChannelId, inscribe::InscriptionOp},
};
use tokio::sync::RwLock;
use url::Url;

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
                config
                    .bedrock_client_config
                    .auth
                    .clone()
                    .map(|auth| BasicAuthCredentials::new(auth.0, auth.1)),
                Url::parse(&config.bedrock_client_config.addr)
                    .context("Bedrock node addr is not a valid url")?,
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
                    .get_block_by_id(
                        header_id,
                        self.config.start_delay_millis,
                        self.config.max_retries,
                    )
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

    pub async fn send_message_to_sequencer(
        &self,
        message: Message,
    ) -> Result<PostIndexerMessageResponse> {
        Ok(self.sequencer_client.post_indexer_message(message).await?)
    }
}

fn parse_blocks(
    block_txs: impl Iterator<Item = SignedMantleTx>,
    decoded_channel_id: &ChannelId,
) -> Vec<HashableBlockData> {
    block_txs
        .flat_map(|tx| {
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
        .collect()
}

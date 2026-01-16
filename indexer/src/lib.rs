use std::sync::Arc;

use anyhow::Result;
use bedrock_client::{BasicAuthCredentials, BedrockClient};
use common::block::HashableBlockData;
use futures::StreamExt;
use log::info;
use nomos_core::mantle::{
    Op, SignedMantleTx,
    ops::channel::{ChannelId, inscribe::InscriptionOp},
};
use tokio::sync::{RwLock, mpsc::Sender};
use tokio_retry::Retry;
use url::Url;

use crate::{config::IndexerConfig, message::IndexerToSequencerMessage, state::IndexerState};

pub mod config;
pub mod message;
pub mod state;

pub struct IndexerCore {
    pub bedrock_client: BedrockClient,
    pub channel_sender: Sender<IndexerToSequencerMessage>,
    pub config: IndexerConfig,
    pub bedrock_url: Url,
    pub channel_id: ChannelId,
    pub state: IndexerState,
}

impl IndexerCore {
    pub fn new(
        addr: &str,
        auth: Option<BasicAuthCredentials>,
        sender: Sender<IndexerToSequencerMessage>,
        config: IndexerConfig,
        channel_id: ChannelId,
    ) -> Result<Self> {
        Ok(Self {
            bedrock_client: BedrockClient::new(auth)?,
            bedrock_url: Url::parse(addr)?,
            channel_sender: sender,
            config,
            channel_id,
            // No state setup for now, future task.
            state: IndexerState {
                latest_seen_block: Arc::new(RwLock::new(0)),
            },
        })
    }

    pub async fn subscribe_parse_block_stream(&self) -> Result<()> {
        loop {
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

                // Simple retry strategy on requests
                let strategy =
                    tokio_retry::strategy::FibonacciBackoff::from_millis(self.config.start_delay)
                        .take(self.config.limit_retry);

                if let Some(l1_block) = Retry::spawn(strategy, || {
                    self.bedrock_client
                        .0
                        .get_block_by_id(self.bedrock_url.clone(), header_id)
                })
                .await?
                {
                    info!("Extracted L1 block at height {}", block_info.height);

                    let l2_blocks_parsed =
                        parse_blocks(l1_block.into_transactions().into_iter(), &self.channel_id);

                    for l2_block in l2_blocks_parsed {
                        // State modification, will be updated in future
                        {
                            let mut guard = self.state.latest_seen_block.write().await;
                            if l2_block.block_id > *guard {
                                *guard = l2_block.block_id;
                            }
                        }

                        // Sending data into sequencer, may need to be expanded.
                        let message = IndexerToSequencerMessage::BlockObserved {
                            l1_block_id: block_info.height,
                            l2_block_height: l2_block.block_id,
                        };

                        self.channel_sender.send(message.clone()).await?;

                        info!("Sent message {:#?} to sequencer", message);
                    }
                }
            }

            // Refetch stream after delay
            tokio::time::sleep(std::time::Duration::from_millis(
                self.config.resubscribe_interval,
            ))
            .await;
        }
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
                        borsh::from_slice::<HashableBlockData>(inscription).ok()
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

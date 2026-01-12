use anyhow::Result;
use bedrock_client::{BasicAuthCredentials, BedrockClient};
use common::block::HashableBlockData;
use futures::StreamExt;
use nomos_core::mantle::{
    Op, SignedMantleTx,
    ops::channel::{ChannelId, inscribe::InscriptionOp},
};
use tokio::sync::mpsc::Sender;
use url::Url;

use crate::config::IndexerConfig;

pub mod config;

pub struct IndexerCore {
    pub bedrock_client: BedrockClient,
    pub bedrock_url: Url,
    pub channel_sender: Sender<HashableBlockData>,
    pub config: IndexerConfig,
}

impl IndexerCore {
    pub fn new(
        addr: &str,
        auth: Option<BasicAuthCredentials>,
        sender: Sender<HashableBlockData>,
        config: IndexerConfig,
    ) -> Result<Self> {
        Ok(Self {
            bedrock_client: BedrockClient::new(auth)?,
            bedrock_url: Url::parse(addr)?,
            channel_sender: sender,
            config,
        })
    }

    pub async fn subscribe_parse_block_stream(&self) -> Result<()> {
        let mut stream_pinned = Box::pin(
            self.bedrock_client
                .0
                .get_lib_stream(self.bedrock_url.clone())
                .await?,
        );

        while let Some(block_info) = stream_pinned.next().await {
            let header_id = block_info.header_id;

            if let Some(l1_block) = self
                .bedrock_client
                .0
                .get_block_by_id(self.bedrock_url.clone(), header_id)
                .await?
            {
                let l2_blocks_parsed = parse_blocks(
                    l1_block.into_transactions().into_iter(),
                    &self.config.channel_id,
                );

                for l2_block in l2_blocks_parsed {
                    self.channel_sender.send(l2_block).await?;
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

use anyhow::Result;
use bedrock_client::BedrockClient;
use common::block::{Block, HashableBlockData};
// ToDo: Remove after testnet
use common::{HashType, PINATA_BASE58};
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
    pub config: IndexerConfig,
    pub store: IndexerStore,
}

impl IndexerCore {
    pub fn new(config: IndexerConfig) -> Result<Self> {
        // ToDo: replace with correct startup
        let hashable_data = HashableBlockData {
            block_id: 1,
            transactions: vec![],
            prev_block_hash: HashType([0; 32]),
            timestamp: 0,
        };

        let signing_key = nssa::PrivateKey::try_new(config.signing_key).unwrap();
        let channel_genesis_msg_id = [0; 32];
        let start_block = hashable_data.into_pending_block(&signing_key, channel_genesis_msg_id);

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
            .map(|acc_data| (acc_data.account_id, acc_data.balance))
            .collect();

        let mut state = nssa::V02State::new_with_genesis_accounts(&init_accs, &initial_commitments);

        // ToDo: Remove after testnet
        state.add_pinata_program(PINATA_BASE58.parse().unwrap());

        let home = config.home.join("rocksdb");

        Ok(Self {
            bedrock_client: BedrockClient::new(
                config.bedrock_client_config.backoff,
                config.bedrock_client_config.addr.clone(),
                config.bedrock_client_config.auth.clone(),
            )?,
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
                    .get_block_by_id(header_id)
                    .await?
                {
                    info!("Extracted L1 block at height {}", block_info.height);

                    let l2_blocks_parsed = parse_blocks(
                        l1_block.into_transactions().into_iter(),
                        &self.config.channel_id,
                    ).collect::<Vec<_>>();

                    let mut l2_blocks_parsed_ids: Vec<_> = l2_blocks_parsed.iter().map(|block| block.header.block_id).collect();
                    l2_blocks_parsed_ids.sort();
                    info!("Parsed {} L2 blocks with ids {:?}", l2_blocks_parsed.len(), l2_blocks_parsed_ids);

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

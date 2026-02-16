use std::collections::VecDeque;

use anyhow::Result;
use bedrock_client::{BedrockClient, HeaderId};
use common::block::{Block, HashableBlockData};
// ToDo: Remove after testnet
use common::{HashType, PINATA_BASE58};
use futures::StreamExt;
use log::info;
use logos_blockchain_core::mantle::{
    Op, SignedMantleTx,
    ops::channel::{ChannelId, MsgId, inscribe::InscriptionOp},
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
            let last_l1_header = self.store.last_observed_l1_header()?;

            let mut last_fin_header = match last_l1_header {
                Some(last_l1_header) => {
                    last_l1_header
                },
                None => {
                    info!("Searching for the start of a channel");

                    let start_buff = self.search_for_channel_start().await?;

                    let last_l1_header = start_buff.back().ok_or(anyhow::anyhow!("Failure: Chain is empty"))?.header().id();

                    for l1_block in start_buff {
                        info!("Observed L1 block at height {}", l1_block.header().slot().into_inner());

                        let curr_l1_header = l1_block.header().id();

                        let l2_blocks_parsed = parse_blocks(
                            l1_block.into_transactions().into_iter(),
                            &self.config.channel_id,
                        ).collect::<Vec<_>>();

                        info!("Parsed {} L2 blocks", l2_blocks_parsed.len());

                        for l2_block in l2_blocks_parsed {
                            self.store.put_block(l2_block.clone(), curr_l1_header)?;

                            yield Ok(l2_block);
                        }
                    }

                    last_l1_header
                },
            };

            loop {
                let buff = self.rollback_to_last_known_finalized_l1_id(last_fin_header).await?;

                last_fin_header = buff.back().ok_or(anyhow::anyhow!("Failure: Chain is empty"))?.header().id();

                for l1_block in buff {
                    info!("Observed L1 block at height {}", l1_block.header().slot().into_inner());

                    let curr_l1_header = l1_block.header().id();

                    let l2_blocks_parsed = parse_blocks(
                        l1_block.into_transactions().into_iter(),
                        &self.config.channel_id,
                    ).collect::<Vec<_>>();

                    let mut l2_blocks_parsed_ids: Vec<_> = l2_blocks_parsed.iter().map(|block| block.header.block_id).collect();
                    l2_blocks_parsed_ids.sort();
                    info!("Parsed {} L2 blocks with ids {:?}", l2_blocks_parsed.len(), l2_blocks_parsed_ids);

                    for l2_block in l2_blocks_parsed {
                        self.store.put_block(l2_block.clone(), curr_l1_header)?;

                        yield Ok(l2_block);
                    }
                }
            }
        }
    }

    async fn wait_last_finalized_block_header(&self) -> Result<HeaderId> {
        let mut stream_pinned = Box::pin(self.bedrock_client.get_lib_stream().await?);
        stream_pinned
            .next()
            .await
            .ok_or(anyhow::anyhow!("Stream failure"))
            .map(|info| info.header_id)
    }

    /// WARNING: depending on chain behaviour,
    /// may take indefinite amount of time
    pub async fn search_for_channel_start(
        &self,
    ) -> Result<VecDeque<bedrock_client::Block<SignedMantleTx>>> {
        let mut curr_last_header = self.wait_last_finalized_block_header().await?;
        // Storing start for future use
        let mut rollback_start = curr_last_header;
        // ToDo: How to get root?
        let mut rollback_limit = HeaderId::from([0; 32]);
        // ToDo: Not scalable, initial buffer should be stored in DB to not run out of memory
        // Don't want to complicate DB even more right now.
        let mut block_buffer = VecDeque::new();

        'outer: loop {
            loop {
                // let res = self
                //     .bedrock_client
                //     .get_block_by_id(curr_last_header)
                //     .await?;

                // let curr_last_block;

                // match res {
                //     Some(block) => {curr_last_block = block},
                //     None => {
                //         break;
                //     }
                // }

                let Some(curr_last_block) = self
                    .bedrock_client
                    .get_block_by_id(curr_last_header)
                    .await?
                else {
                    log::error!("Failed to get block for header {curr_last_header}");
                    return Err(anyhow::anyhow!("Chain inconsistency"));
                };

                info!(
                    "INITIAL_SEARCH: Observed L1 block at height {}",
                    curr_last_block.header().slot().into_inner()
                );
                info!(
                    "INITIAL_SEARCH: This block header is {}",
                    curr_last_block.header().id()
                );
                info!(
                    "INITIAL_SEARCH: This block parent is {}",
                    curr_last_block.header().parent()
                );

                block_buffer.push_front(curr_last_block.clone());

                if let Some(_) = curr_last_block.transactions().find_map(|tx| {
                    tx.mantle_tx.ops.iter().find_map(|op| match op {
                        Op::ChannelInscribe(InscriptionOp {
                            channel_id, parent, ..
                        }) => {
                            if (channel_id == &self.config.channel_id) && (parent == &MsgId::root())
                            {
                                Some(curr_last_block.header().id())
                            } else {
                                None
                            }
                        }
                        _ => None,
                    })
                }) {
                    info!("INITIAL_SEARCH: Found channel start");
                    break 'outer;
                } else {
                    // Step back to parent
                    let parent = curr_last_block.header().parent();

                    if parent == rollback_limit {
                        break;
                    }

                    curr_last_header = parent;
                };
            }

            info!("INITIAL_SEARCH: Reached rollback limit, refetching last block");

            block_buffer.clear();
            rollback_limit = rollback_start;
            curr_last_header = self.wait_last_finalized_block_header().await?;
            rollback_start = curr_last_header;
        }

        Ok(block_buffer)
    }

    pub async fn rollback_to_last_known_finalized_l1_id(
        &self,
        last_fin_header: HeaderId,
    ) -> Result<VecDeque<bedrock_client::Block<SignedMantleTx>>> {
        let mut curr_last_header = self.wait_last_finalized_block_header().await?;
        // ToDo: Not scalable, buffer should be stored in DB to not run out of memory
        // Don't want to complicate DB even more right now.
        let mut block_buffer = VecDeque::new();

        loop {
            let Some(curr_last_block) = self
                .bedrock_client
                .get_block_by_id(curr_last_header)
                .await?
            else {
                return Err(anyhow::anyhow!("Chain inconsistency"));
            };

            if curr_last_block.header().id() == last_fin_header {
                break;
            } else {
                // Step back to parent
                curr_last_header = curr_last_block.header().parent();
            }

            block_buffer.push_front(curr_last_block.clone());
        }

        Ok(block_buffer)
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

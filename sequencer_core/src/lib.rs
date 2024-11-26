use anyhow::Result;
use config::SequencerConfig;
use mempool::MemPool;
use sequecer_store::SequecerChainStore;
use storage::block::{Block, HashableBlockData};
use transaction_mempool::TransactionMempool;

pub mod config;
pub mod sequecer_store;
pub mod transaction_mempool;

pub struct SequencerCore {
    pub store: SequecerChainStore,
    pub mempool: MemPool<TransactionMempool>,
    pub sequencer_config: SequencerConfig,
    pub chain_height: u64,
}

impl SequencerCore {
    pub fn start_from_config(config: SequencerConfig) -> Self {
        Self {
            store: SequecerChainStore::new_with_genesis(
                &config.home,
                config.genesis_id,
                config.is_genesis_random,
            ),
            mempool: MemPool::<TransactionMempool>::default(),
            chain_height: config.genesis_id,
            sequencer_config: config,
        }
    }

    ///Produces new block from transaction outputs in mempool
    pub fn produce_new_block_simple(&mut self) -> Result<()> {
        let transactions = self
            .mempool
            .pop_size(self.sequencer_config.max_num_tx_in_block);

        let hashable_data = HashableBlockData {
            block_id: self.chain_height + 1,
            transactions: transactions.into_iter().map(|tx_mem| tx_mem.tx).collect(),
            data: vec![],
        };

        let block = Block::produce_block_from_hashable_data(hashable_data);

        self.store.block_store.put_block_at_id(block)?;

        self.chain_height += 1;

        Ok(())
    }
}

use std::{pin::pin, sync::Arc};

use anyhow::{Context as _, Result, anyhow};
use futures::StreamExt as _;
use indexer_core::{IndexerCore, config::IndexerConfig};
use indexer_service_protocol::{Account, AccountId, Block, BlockId, Hash, Transaction};
use jsonrpsee::{
    SubscriptionMessage, SubscriptionSink,
    core::{Serialize, SubscriptionResult},
    types::ErrorObjectOwned,
};
use serde_json::value::RawValue;
use tokio::sync::{Mutex, broadcast};

pub struct IndexerService {
    service_impl: Arc<Mutex<IndexerServiceImpl>>,
    respond_subscribers_loop_handle: tokio::task::JoinHandle<Result<()>>,
}
impl Drop for IndexerService {
    fn drop(&mut self) {
        self.respond_subscribers_loop_handle.abort();
    }
}

impl IndexerService {
    pub fn new(config: IndexerConfig) -> Result<Self> {
        let service_impl = Arc::new(Mutex::new(IndexerServiceImpl::new(IndexerCore::new(
            config,
        )?)));

        let respond_subscribers_loop_handle = tokio::spawn(
            IndexerServiceImpl::respond_subscribers_loop(Arc::clone(&service_impl)),
        );

        Ok(Self {
            service_impl,
            respond_subscribers_loop_handle,
        })
    }
}

#[async_trait::async_trait]
impl indexer_service_rpc::RpcServer for IndexerService {
    async fn subscribe_to_finalized_blocks(
        &self,
        subscription_sink: jsonrpsee::PendingSubscriptionSink,
    ) -> SubscriptionResult {
        let mut rx = self
            .service_impl
            .lock()
            .await
            .finalized_block_id_tx
            .subscribe();

        let sink = subscription_sink.accept().await?;

        tokio::spawn(async move {
            while let Ok(block_id) = rx.recv().await {
                let msg = SubscriptionMessage::from(
                    RawValue::from_string(block_id.to_string())
                        .expect("u64 string is always valid JSON"),
                );
                if sink.send(msg).await.is_err() {
                    break;
                }
            }
        });

        Ok(())
    }

    async fn get_block_by_id(&self, _block_id: BlockId) -> Result<Block, ErrorObjectOwned> {
        todo!()
    }

    async fn get_block_by_hash(&self, _block_hash: Hash) -> Result<Block, ErrorObjectOwned> {
        todo!()
    }

    async fn get_account(&self, _account_id: AccountId) -> Result<Account, ErrorObjectOwned> {
        todo!()
    }

    async fn get_transaction(&self, _tx_hash: Hash) -> Result<Transaction, ErrorObjectOwned> {
        todo!()
    }

    async fn get_blocks(&self, _offset: u32, _limit: u32) -> Result<Vec<Block>, ErrorObjectOwned> {
        todo!()
    }

    async fn get_transactions_by_account(
        &self,
        _account_id: AccountId,
        _limit: u32,
        _offset: u32,
    ) -> Result<Vec<Transaction>, ErrorObjectOwned> {
        todo!()
    }
}

struct IndexerServiceImpl {
    indexer: IndexerCore,
    finalized_block_id_tx: broadcast::Sender<BlockId>,
}

impl IndexerServiceImpl {
    fn new(indexer: IndexerCore) -> Self {
        let (finalized_block_id_tx, _block_rx) = broadcast::channel(1024);

        Self {
            indexer,
            finalized_block_id_tx,
        }
    }

    async fn respond_subscribers_loop(service_impl: Arc<Mutex<IndexerServiceImpl>>) -> Result<()> {
        let indexer_clone = service_impl.lock().await.indexer.clone();

        let mut block_stream = pin!(indexer_clone.subscribe_parse_block_stream().await);
        while let Some(block) = block_stream.next().await {
            let block = block.context("Failed to get L2 block data")?;

            // Cloning subscriptions to avoid holding the lock while sending
            service_impl
                .lock()
                .await
                .finalized_block_id_tx
                .send(block.header.block_id)?;
        }

        Err(anyhow!("Block stream ended unexpectedly"))
    }
}

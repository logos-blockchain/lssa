use std::{pin::pin, sync::Arc};

use anyhow::{Context as _, Result, anyhow};
use futures::StreamExt as _;
use indexer_core::{IndexerCore, config::IndexerConfig};
use indexer_service_protocol::{Account, AccountId, Block, BlockId, Hash, Transaction};
use jsonrpsee::{SubscriptionSink, core::{Serialize, SubscriptionResult}, types::ErrorObjectOwned};
use tokio::sync::Mutex;

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
        let service_impl = Arc::new(Mutex::new(IndexerServiceImpl::new(
            IndexerCore::new(config)?,
        )));

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
        let sink = subscription_sink.accept().await?;
        self.service_impl.lock().await.add_subscription(Subscription::new(sink)).await;
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
    subscriptions: Vec<Subscription<Block>>,
}

impl IndexerServiceImpl {
    fn new(indexer: IndexerCore) -> Self {
        Self {
            indexer,
            subscriptions: Vec::new(),
        }
    }

    async fn add_subscription(&mut self, subscription: Subscription<Block>) {
        self.subscriptions.push(subscription);
    }

    async fn respond_subscribers_loop(service_impl: Arc<Mutex<IndexerServiceImpl>>) -> Result<()> {
        let indexer_clone = service_impl.lock().await.indexer.clone();

        let mut block_stream = pin!(indexer_clone.subscribe_parse_block_stream().await);
        while let Some(block) = block_stream.next().await {
            let block= block.context("Failed to get L2 block data")?;
            let block = block.try_into().context("Failed to convert L2 Block into protocol Block")?;

            // Cloning subscriptions to avoid holding the lock while sending
            let subscriptions = service_impl.lock().await.subscriptions.clone();
            for sink in subscriptions {
                sink.send(&block).await?;
            }
        }

        Err(anyhow!("Block stream ended unexpectedly"))
    }
}

#[derive(Clone)]
struct Subscription<T> {
    sink: SubscriptionSink,
    _marker: std::marker::PhantomData<T>,
}

impl<T> Subscription<T> {
    fn new(sink: SubscriptionSink) -> Self {
        Self {
            sink,
            _marker: std::marker::PhantomData,
        }
    }

    async fn send(&self, item: &T) -> Result<()>
    where T: Serialize
    {
        let json = serde_json::value::to_raw_value(item)
            .context("Failed to serialize item for subscription")?;
        self.sink.send(json).await?;
        Ok(())
    }
}

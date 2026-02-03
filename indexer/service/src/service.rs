use std::{pin::pin, sync::Arc};

use anyhow::{Context as _, Result, bail};
use futures::StreamExt as _;
use indexer_core::{IndexerCore, config::IndexerConfig};
use indexer_service_protocol::{Account, AccountId, Block, BlockId, Hash, Transaction};
use jsonrpsee::{
    SubscriptionSink,
    core::{Serialize, SubscriptionResult},
    types::ErrorObjectOwned,
};
use tokio::sync::{Mutex, mpsc::UnboundedSender};

pub struct IndexerService {
    subscription_service: SubscriptionService,

    #[expect(
        dead_code,
        reason = "Will be used in future implementations of RPC methods"
    )]
    indexer: IndexerCore,
}

impl IndexerService {
    pub async fn new(config: IndexerConfig) -> Result<Self> {
        let indexer = IndexerCore::new(config).await?;
        let subscription_service = SubscriptionService::spawn_new(indexer.clone());

        Ok(Self {
            subscription_service,
            indexer,
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
        self.subscription_service
            .add_subscription(Subscription::new(sink))?;

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

struct SubscriptionService {
    respond_subscribers_loop_handle: tokio::task::JoinHandle<Result<()>>,
    new_subscription_sender: UnboundedSender<Subscription<BlockId>>,
}

impl SubscriptionService {
    pub fn spawn_new(indexer: IndexerCore) -> Self {
        let (new_subscription_sender, mut sub_receiver) =
            tokio::sync::mpsc::unbounded_channel::<Subscription<BlockId>>();

        let subscriptions = Arc::new(Mutex::new(Vec::new()));

        let respond_subscribers_loop_handle = tokio::spawn(async move {
            let mut block_stream = pin!(indexer.subscribe_parse_block_stream().await);

            loop {
                tokio::select! {
                    sub = sub_receiver.recv() => {
                        let Some(subscription) = sub else {
                            bail!("Subscription receiver closed unexpectedly");
                        };
                        subscriptions.lock().await.push(subscription);
                    }
                    block_opt = block_stream.next() => {
                        let Some(block) = block_opt else {
                            bail!("Block stream ended unexpectedly");
                        };
                        let block = block.context("Failed to get L2 block data")?;
                        let block: indexer_service_protocol::Block = block
                            .try_into()
                            .context("Failed to convert L2 Block into protocol Block")?;

                        // Cloning subscriptions to avoid holding the lock while sending
                        let subscriptions = subscriptions.lock().await.clone();
                        for sink in subscriptions {
                            sink.send(&block.header.block_id).await?;
                        }
                    }
                }
            }
        });

        Self {
            respond_subscribers_loop_handle,
            new_subscription_sender,
        }
    }

    pub fn add_subscription(&self, subscription: Subscription<BlockId>) -> Result<()> {
        self.new_subscription_sender.send(subscription)?;
        Ok(())
    }
}

impl Drop for SubscriptionService {
    fn drop(&mut self) {
        self.respond_subscribers_loop_handle.abort();
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
    where
        T: Serialize,
    {
        let json = serde_json::value::to_raw_value(item)
            .context("Failed to serialize item for subscription")?;
        self.sink.send(json).await?;
        Ok(())
    }
}

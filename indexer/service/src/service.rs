use indexer_service_protocol::{Account, AccountId, Block, BlockId, Hash, Transaction};
use jsonrpsee::{core::SubscriptionResult, types::ErrorObjectOwned};

pub struct IndexerService;

// `async_trait` is required by `jsonrpsee`
#[async_trait::async_trait]
impl indexer_service_rpc::RpcServer for IndexerService {
    async fn subscribe_to_blocks(
        &self,
        _subscription_sink: jsonrpsee::PendingSubscriptionSink,
        _from: BlockId,
    ) -> SubscriptionResult {
        todo!()
    }

    async fn get_block_by_id(&self, _block_id: BlockId) -> Result<Block, ErrorObjectOwned> {
        todo!()
    }

    async fn get_block_by_hash(&self, _block_hash: Hash) -> Result<Block, ErrorObjectOwned> {
        todo!()
    }

    async fn get_last_block_id(&self) -> Result<BlockId, ErrorObjectOwned> {
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

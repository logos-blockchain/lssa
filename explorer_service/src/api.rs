use indexer_service_protocol::{Account, AccountId, Block, BlockId, HashType, Transaction};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Search results structure
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchResults {
    pub blocks: Vec<Block>,
    pub transactions: Vec<Transaction>,
    pub accounts: Vec<(AccountId, Option<Account>)>,
}

/// RPC client type
#[cfg(feature = "ssr")]
pub type IndexerRpcClient = jsonrpsee::http_client::HttpClient;

/// Get account information by ID
#[server]
pub async fn get_account(account_id: AccountId) -> Result<Account, ServerFnError> {
    use indexer_service_rpc::RpcClient as _;
    let client = expect_context::<IndexerRpcClient>();
    client
        .get_account(account_id)
        .await
        .map_err(|e| ServerFnError::ServerError(format!("RPC error: {}", e)))
}

/// Parse hex string to bytes
#[cfg(feature = "ssr")]
fn parse_hex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim().trim_start_matches("0x");
    hex::decode(s).ok()
}

/// Search for a block, transaction, or account by query string
#[server]
pub async fn search(query: String) -> Result<SearchResults, ServerFnError> {
    use indexer_service_rpc::RpcClient as _;
    let client = expect_context::<IndexerRpcClient>();

    let mut blocks = Vec::new();
    let mut transactions = Vec::new();
    let mut accounts = Vec::new();

    // Try to parse as hash (32 bytes)
    if let Some(bytes) = parse_hex(&query)
        && let Ok(hash_array) = <[u8; 32]>::try_from(bytes)
    {
        let hash = HashType(hash_array);

        // Try as block hash
        if let Ok(block) = client.get_block_by_hash(hash).await {
            blocks.push(block);
        }

        // Try as transaction hash
        if let Ok(tx) = client.get_transaction(hash).await {
            transactions.push(tx);
        }

        // Try as account ID
        let account_id = AccountId { value: hash_array };
        match client.get_account(account_id).await {
            Ok(account) => {
                accounts.push((account_id, Some(account)));
            }
            Err(_) => {
                // Account might not exist yet, still add it to results
                accounts.push((account_id, None));
            }
        }
    }

    // Try as block ID
    if let Ok(block_id) = query.parse::<u64>()
        && let Ok(block) = client.get_block_by_id(block_id).await
    {
        blocks.push(block);
    }

    Ok(SearchResults {
        blocks,
        transactions,
        accounts,
    })
}

/// Get block by ID
#[server]
pub async fn get_block_by_id(block_id: BlockId) -> Result<Block, ServerFnError> {
    use indexer_service_rpc::RpcClient as _;
    let client = expect_context::<IndexerRpcClient>();
    client
        .get_block_by_id(block_id)
        .await
        .map_err(|e| ServerFnError::ServerError(format!("RPC error: {}", e)))
}

/// Get block by hash
#[server]
pub async fn get_block_by_hash(block_hash: HashType) -> Result<Block, ServerFnError> {
    use indexer_service_rpc::RpcClient as _;
    let client = expect_context::<IndexerRpcClient>();
    client
        .get_block_by_hash(block_hash)
        .await
        .map_err(|e| ServerFnError::ServerError(format!("RPC error: {}", e)))
}

/// Get transaction by hash
#[server]
pub async fn get_transaction(tx_hash: HashType) -> Result<Transaction, ServerFnError> {
    use indexer_service_rpc::RpcClient as _;
    let client = expect_context::<IndexerRpcClient>();
    client
        .get_transaction(tx_hash)
        .await
        .map_err(|e| ServerFnError::ServerError(format!("RPC error: {}", e)))
}

/// Get blocks with pagination
#[server]
pub async fn get_blocks(offset: u32, limit: u32) -> Result<Vec<Block>, ServerFnError> {
    use indexer_service_rpc::RpcClient as _;
    let client = expect_context::<IndexerRpcClient>();
    client
        .get_blocks(offset, limit)
        .await
        .map_err(|e| ServerFnError::ServerError(format!("RPC error: {}", e)))
}

/// Get transactions by account
#[server]
pub async fn get_transactions_by_account(
    account_id: AccountId,
    limit: u32,
    offset: u32,
) -> Result<Vec<Transaction>, ServerFnError> {
    use indexer_service_rpc::RpcClient as _;
    let client = expect_context::<IndexerRpcClient>();
    client
        .get_transactions_by_account(account_id, limit, offset)
        .await
        .map_err(|e| ServerFnError::ServerError(format!("RPC error: {}", e)))
}

/// Create the RPC client for the indexer service (server-side only)
#[cfg(feature = "ssr")]
pub fn create_indexer_rpc_client(url: &url::Url) -> Result<IndexerRpcClient, String> {
    use jsonrpsee::http_client::HttpClientBuilder;
    use log::info;

    info!("Connecting to Indexer RPC on URL: {url}");

    HttpClientBuilder::default()
        .build(url.as_str())
        .map_err(|e| format!("Failed to create RPC client: {e}"))
}

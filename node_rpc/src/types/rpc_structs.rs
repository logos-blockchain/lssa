use rpc_primitives::errors::RpcParseError;
use rpc_primitives::parse_request;
use rpc_primitives::parser::parse_params;
use rpc_primitives::parser::RpcRequest;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use storage::block::Block;
use storage::transaction::Transaction;

#[derive(Serialize, Deserialize, Debug)]
pub struct RegisterAccountRequest {}

#[derive(Serialize, Deserialize, Debug)]
pub struct SendTxRequest {
    pub transaction: Transaction,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetBlockDataRequest {
    pub block_id: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ExecuteSubscenarioRequest {
    pub scenario_id: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetGenesisIdRequest {}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetLastBlockRequest {}

parse_request!(RegisterAccountRequest);
parse_request!(SendTxRequest);
parse_request!(GetBlockDataRequest);
parse_request!(GetGenesisIdRequest);
parse_request!(ExecuteSubscenarioRequest);
parse_request!(GetLastBlockRequest);

#[derive(Serialize, Deserialize, Debug)]
pub struct HelloResponse {
    pub greeting: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RegisterAccountResponse {
    pub status: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SendTxResponse {
    pub status: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetBlockDataResponse {
    pub block: Block,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ExecuteSubscenarioResponse {
    pub scenario_result: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetGenesisIdResponse {
    pub genesis_id: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetLastBlockResponse {
    pub last_block: u64,
}

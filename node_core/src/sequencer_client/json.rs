use serde::{Deserialize, Serialize};
use storage::{block::Block, transaction::Transaction};

//Requests

#[derive(Serialize, Deserialize, Debug)]
pub struct RegisterAccountRequest {
    pub nullifier_public_key: Vec<u8>,
    pub viewing_public_key: Vec<u8>,
    pub address: [u8; 32],
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SendTxRequest {
    pub transaction: Transaction,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetBlockDataRequest {
    pub block_id: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetGenesisIdRequest {}

//Responses

#[derive(Serialize, Deserialize, Debug)]
pub struct RegisterAccountResponse {
    pub status: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SendTxResponse {
    pub status: String,
    pub additional_data: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetBlockDataResponse {
    pub block: Block,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetGenesisIdResponse {
    pub genesis_id: u64,
}

//General

#[derive(Debug, Clone, Serialize)]
pub struct SequencerRpcRequest {
    jsonrpc: String,
    pub method: String,
    pub params: serde_json::Value,
    pub id: u64,
}

impl SequencerRpcRequest {
    pub fn from_payload_version_2_0(method: String, payload: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method,
            params: payload,
            //ToDo: Correct checking of id
            id: 1,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SequencerRpcResponse {
    pub jsonrpc: String,
    pub result: serde_json::Value,
    pub id: u64,
}

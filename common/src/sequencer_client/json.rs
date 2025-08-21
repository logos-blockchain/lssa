use serde::{Deserialize, Serialize};

//Requests

#[derive(Serialize, Deserialize, Debug)]
pub struct SendTxRequest {
    pub transaction: Vec<u8>,
}

//Responses

#[derive(Serialize, Deserialize, Debug)]
pub struct SendTxResponse {
    pub status: String,
    pub tx_hash: String,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
///Helperstruct for account serialization
pub struct AccountInitialData {
    ///Hex encoded `AccountAddress`
    pub addr: String,
    pub balance: u64,
}

use crate::block::Block;
use crate::parse_request;
use crate::transaction::Transaction;

use super::errors::RpcParseError;
use super::parser::parse_params;
use super::parser::RpcRequest;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Debug)]
pub struct HelloRequest {}

#[derive(Serialize, Deserialize, Debug)]
pub struct RegisterAccountRequest {
    pub address: [u8; 32],
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SendTxRequest {
    pub transaction: Transaction,
    ///UTXO Commitment Root, Pub Tx Root
    pub tx_roots: [[u8; 32]; 2],
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetBlockDataRequest {
    pub block_id: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetGenesisIdRequest {}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetLastBlockRequest {}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetAccountBalanceRequest {
    pub address: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetTransactionByHashRequest {
    pub hash: String,
}

parse_request!(HelloRequest);
parse_request!(RegisterAccountRequest);
parse_request!(SendTxRequest);
parse_request!(GetBlockDataRequest);
parse_request!(GetGenesisIdRequest);
parse_request!(GetLastBlockRequest);
parse_request!(GetAccountBalanceRequest);
parse_request!(GetTransactionByHashRequest);

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
pub struct GetGenesisIdResponse {
    pub genesis_id: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetLastBlockResponse {
    pub last_block: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetAccountBalanceResponse {
    pub balance: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GetTransactionByHashResponse {
    pub transaction: Option<Transaction>,
}

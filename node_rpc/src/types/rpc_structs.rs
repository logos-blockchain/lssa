use common::parse_request;
use common::rpc_primitives::errors::RpcParseError;
use common::rpc_primitives::parser::parse_params;
use common::rpc_primitives::parser::RpcRequest;
use common::transaction::TxKind;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Debug)]
pub struct ExecuteSubscenarioRequest {
    pub scenario_id: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ExecuteScenarioSplitRequest {
    pub visibility_list: [bool; 3],
    pub publication_index: usize,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ExecuteScenarioMultipleSendRequest {
    pub number_of_assets: usize,
    pub number_to_send: usize,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ShowAccountPublicBalanceRequest {
    pub account_addr: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ShowAccountUTXORequest {
    pub account_addr: String,
    pub utxo_hash: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ShowTransactionRequest {
    pub tx_hash: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WriteDepositPublicBalanceRequest {
    pub account_addr: String,
    pub amount: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WriteMintPrivateUTXORequest {
    pub account_addr: String,
    pub amount: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WriteMintPrivateUTXOMultipleAssetsRequest {
    pub account_addr: String,
    pub num_of_assets: usize,
    pub amount: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WriteSendPrivateUTXORequest {
    pub account_addr_sender: String,
    pub account_addr_receiver: String,
    pub utxo_hash: String,
    pub utxo_commitment: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WriteSendShieldedUTXORequest {
    pub account_addr_sender: String,
    pub account_addr_receiver: String,
    pub amount: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WriteSendDeshieldedBalanceRequest {
    pub account_addr_sender: String,
    pub account_addr_receiver: String,
    pub utxo_hash: String,
    pub utxo_commitment: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WriteSplitUTXORequest {
    pub account_addr_sender: String,
    pub account_addr_receivers: [String; 3],
    pub visibility_list: [bool; 3],
    pub utxo_hash: String,
    pub utxo_commitment: String,
}

// parse_request!(GetGenesisIdRequest);
parse_request!(ExecuteSubscenarioRequest);
parse_request!(ExecuteScenarioSplitRequest);
parse_request!(ExecuteScenarioMultipleSendRequest);
// parse_request!(GetLastBlockRequest);

parse_request!(ShowAccountPublicBalanceRequest);
parse_request!(ShowAccountUTXORequest);
parse_request!(ShowTransactionRequest);

parse_request!(WriteDepositPublicBalanceRequest);
parse_request!(WriteMintPrivateUTXORequest);
parse_request!(WriteMintPrivateUTXOMultipleAssetsRequest);
parse_request!(WriteSendPrivateUTXORequest);
parse_request!(WriteSendShieldedUTXORequest);
parse_request!(WriteSendDeshieldedBalanceRequest);
parse_request!(WriteSplitUTXORequest);

#[derive(Serialize, Deserialize, Debug)]
pub struct ExecuteSubscenarioResponse {
    pub scenario_result: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ExecuteScenarioSplitResponse {
    pub scenario_result: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ExecuteScenarioMultipleSendResponse {
    pub scenario_result: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ShowAccountPublicBalanceResponse {
    pub addr: String,
    pub balance: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ShowAccountUTXOResponse {
    pub hash: String,
    pub asset: Vec<u8>,
    pub amount: u128,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ShowTransactionResponse {
    pub hash: String,
    pub tx_kind: TxKind,
    pub public_input: String,
    pub public_output: String,
    pub utxo_commitments_created_hashes: Vec<String>,
    pub utxo_commitments_spent_hashes: Vec<String>,
    pub utxo_nullifiers_created_hashes: Vec<String>,
    pub encoded_data: Vec<(String, String)>,
    pub ephemeral_pub_key: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WriteDepositPublicBalanceResponse {
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UTXOShortEssentialStruct {
    pub hash: String,
    pub commitment_hash: String,
    pub asset: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WriteMintPrivateUTXOResponse {
    pub status: String,
    pub utxo: UTXOShortEssentialStruct,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WriteMintPrivateUTXOMultipleAssetsResponse {
    pub status: String,
    pub utxos: Vec<UTXOShortEssentialStruct>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WriteSendPrivateUTXOResponse {
    pub status: String,
    pub utxo_result: UTXOShortEssentialStruct,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WriteSendShieldedUTXOResponse {
    pub status: String,
    pub utxo_result: UTXOShortEssentialStruct,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WriteSendDeshieldedUTXOResponse {
    pub status: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct WriteSendSplitUTXOResponse {
    pub status: String,
    pub utxo_results: Vec<UTXOShortEssentialStruct>,
}

use std::collections::HashMap;

use actix_web::Error as HttpError;
use base64::{Engine, engine::general_purpose};
use nssa::{self, program::Program};
use sequencer_core::config::AccountInitialData;
use serde_json::Value;

use common::{
    TreeHashType,
    block::HashableBlockData,
    rpc_primitives::{
        errors::RpcError,
        message::{Message, Request},
        parser::RpcRequest,
        requests::{
            GetAccountBalanceRequest, GetAccountBalanceResponse, GetAccountRequest,
            GetAccountResponse, GetAccountsNoncesRequest, GetAccountsNoncesResponse,
            GetInitialTestnetAccountsRequest, GetProgramIdsRequest, GetProgramIdsResponse,
            GetProofForCommitmentRequest, GetProofForCommitmentResponse,
            GetTransactionByHashRequest, GetTransactionByHashResponse,
        },
    },
    transaction::EncodedTransaction,
};

use common::rpc_primitives::requests::{
    GetBlockDataRequest, GetBlockDataResponse, GetGenesisIdRequest, GetGenesisIdResponse,
    GetLastBlockRequest, GetLastBlockResponse, HelloRequest, HelloResponse, SendTxRequest,
    SendTxResponse,
};

use super::{JsonHandler, respond, types::err_rpc::RpcErr};

pub const HELLO: &str = "hello";
pub const SEND_TX: &str = "send_tx";
pub const GET_BLOCK: &str = "get_block";
pub const GET_GENESIS: &str = "get_genesis";
pub const GET_LAST_BLOCK: &str = "get_last_block";
pub const GET_ACCOUNT_BALANCE: &str = "get_account_balance";
pub const GET_TRANSACTION_BY_HASH: &str = "get_transaction_by_hash";
pub const GET_ACCOUNTS_NONCES: &str = "get_accounts_nonces";
pub const GET_ACCOUNT: &str = "get_account";
pub const GET_PROOF_FOR_COMMITMENT: &str = "get_proof_for_commitment";
pub const GET_PROGRAM_IDS: &str = "get_program_ids";

pub const HELLO_FROM_SEQUENCER: &str = "HELLO_FROM_SEQUENCER";

pub const TRANSACTION_SUBMITTED: &str = "Transaction submitted";

pub const GET_INITIAL_TESTNET_ACCOUNTS: &str = "get_initial_testnet_accounts";

impl JsonHandler {
    pub async fn process(&self, message: Message) -> Result<Message, HttpError> {
        let id = message.id();
        if let Message::Request(request) = message {
            let message_inner = self
                .process_request_internal(request)
                .await
                .map_err(|e| e.0);
            Ok(Message::response(id, message_inner))
        } else {
            Ok(Message::error(RpcError::parse_error(
                "JSON RPC Request format was expected".to_owned(),
            )))
        }
    }

    #[allow(clippy::unused_async)]
    ///Example of request processing
    async fn process_temp_hello(&self, request: Request) -> Result<Value, RpcErr> {
        let _hello_request = HelloRequest::parse(Some(request.params))?;

        let helperstruct = HelloResponse {
            greeting: HELLO_FROM_SEQUENCER.to_string(),
        };

        respond(helperstruct)
    }

    async fn process_send_tx(&self, request: Request) -> Result<Value, RpcErr> {
        let send_tx_req = SendTxRequest::parse(Some(request.params))?;
        let tx = borsh::from_slice::<EncodedTransaction>(&send_tx_req.transaction).unwrap();
        let tx_hash = hex::encode(tx.hash());

        {
            let mut state = self.sequencer_state.lock().await;

            state.push_tx_into_mempool_pre_check(tx)?;
        }

        let helperstruct = SendTxResponse {
            status: TRANSACTION_SUBMITTED.to_string(),
            tx_hash,
        };

        respond(helperstruct)
    }

    async fn process_get_block_data(&self, request: Request) -> Result<Value, RpcErr> {
        let get_block_req = GetBlockDataRequest::parse(Some(request.params))?;

        let block = {
            let state = self.sequencer_state.lock().await;

            state
                .store
                .block_store
                .get_block_at_id(get_block_req.block_id)?
        };

        let helperstruct = GetBlockDataResponse {
            block: borsh::to_vec(&HashableBlockData::from(block)).unwrap(),
        };

        respond(helperstruct)
    }

    async fn process_get_genesis(&self, request: Request) -> Result<Value, RpcErr> {
        let _get_genesis_req = GetGenesisIdRequest::parse(Some(request.params))?;

        let genesis_id = {
            let state = self.sequencer_state.lock().await;

            state.store.block_store.genesis_id
        };

        let helperstruct = GetGenesisIdResponse { genesis_id };

        respond(helperstruct)
    }

    async fn process_get_last_block(&self, request: Request) -> Result<Value, RpcErr> {
        let _get_last_block_req = GetLastBlockRequest::parse(Some(request.params))?;

        let last_block = {
            let state = self.sequencer_state.lock().await;

            state.chain_height
        };

        let helperstruct = GetLastBlockResponse { last_block };

        respond(helperstruct)
    }

    /// Returns the initial accounts for testnet
    /// ToDo: Useful only for testnet and needs to be removed later
    async fn get_initial_testnet_accounts(&self, request: Request) -> Result<Value, RpcErr> {
        let _get_initial_testnet_accounts_request =
            GetInitialTestnetAccountsRequest::parse(Some(request.params))?;

        let initial_accounts: Vec<AccountInitialData> = {
            let state = self.sequencer_state.lock().await;

            state.sequencer_config.initial_accounts.clone()
        };

        respond(initial_accounts)
    }

    /// Returns the balance of the account at the given address.
    /// The address must be a valid hex string of the correct length.
    async fn process_get_account_balance(&self, request: Request) -> Result<Value, RpcErr> {
        let get_account_req = GetAccountBalanceRequest::parse(Some(request.params))?;
        let address_bytes = hex::decode(get_account_req.address)
            .map_err(|_| RpcError::invalid_params("invalid hex".to_string()))?;
        let address = nssa::Address::new(
            address_bytes
                .try_into()
                .map_err(|_| RpcError::invalid_params("invalid length".to_string()))?,
        );

        let balance = {
            let state = self.sequencer_state.lock().await;
            let account = state.store.state.get_account_by_address(&address);
            account.balance
        };

        let helperstruct = GetAccountBalanceResponse { balance };

        respond(helperstruct)
    }

    /// Returns the nonces of the accounts at the given addresses.
    /// Each address must be a valid hex string of the correct length.
    async fn process_get_accounts_nonces(&self, request: Request) -> Result<Value, RpcErr> {
        let get_account_nonces_req = GetAccountsNoncesRequest::parse(Some(request.params))?;
        let mut addresses = vec![];
        for address_raw in get_account_nonces_req.addresses {
            let address = address_raw
                .parse::<nssa::Address>()
                .map_err(|e| RpcError::invalid_params(e.to_string()))?;

            addresses.push(address);
        }

        let nonces = {
            let state = self.sequencer_state.lock().await;

            addresses
                .into_iter()
                .map(|addr| state.store.state.get_account_by_address(&addr).nonce)
                .collect()
        };

        let helperstruct = GetAccountsNoncesResponse { nonces };

        respond(helperstruct)
    }

    /// Returns account struct for given address.
    /// Address must be a valid hex string of the correct length.
    async fn process_get_account(&self, request: Request) -> Result<Value, RpcErr> {
        let get_account_nonces_req = GetAccountRequest::parse(Some(request.params))?;

        let address = get_account_nonces_req
            .address
            .parse::<nssa::Address>()
            .map_err(|e| RpcError::invalid_params(e.to_string()))?;

        let account = {
            let state = self.sequencer_state.lock().await;

            state.store.state.get_account_by_address(&address)
        };

        let helperstruct = GetAccountResponse { account };

        respond(helperstruct)
    }

    /// Returns the transaction corresponding to the given hash, if it exists in the blockchain.
    /// The hash must be a valid hex string of the correct length.
    async fn process_get_transaction_by_hash(&self, request: Request) -> Result<Value, RpcErr> {
        let get_transaction_req = GetTransactionByHashRequest::parse(Some(request.params))?;
        let bytes: Vec<u8> = hex::decode(get_transaction_req.hash)
            .map_err(|_| RpcError::invalid_params("invalid hex".to_string()))?;
        let hash: TreeHashType = bytes
            .try_into()
            .map_err(|_| RpcError::invalid_params("invalid length".to_string()))?;

        let transaction = {
            let state = self.sequencer_state.lock().await;
            state
                .store
                .block_store
                .get_transaction_by_hash(hash)
                .map(|tx| borsh::to_vec(&tx).unwrap())
        };
        let base64_encoded = transaction.map(|tx| general_purpose::STANDARD.encode(tx));
        let helperstruct = GetTransactionByHashResponse {
            transaction: base64_encoded,
        };
        respond(helperstruct)
    }

    /// Returns the commitment proof, corresponding to commitment
    async fn process_get_proof_by_commitment(&self, request: Request) -> Result<Value, RpcErr> {
        let get_proof_req = GetProofForCommitmentRequest::parse(Some(request.params))?;

        let membership_proof = {
            let state = self.sequencer_state.lock().await;
            state
                .store
                .state
                .get_proof_for_commitment(&get_proof_req.commitment)
        };
        let helperstruct = GetProofForCommitmentResponse { membership_proof };
        respond(helperstruct)
    }

    async fn process_get_program_ids(&self, request: Request) -> Result<Value, RpcErr> {
        let _get_proof_req = GetProgramIdsRequest::parse(Some(request.params))?;

        let mut program_ids = HashMap::new();
        program_ids.insert(
            "authenticated_transfer".to_string(),
            Program::authenticated_transfer_program().id(),
        );
        program_ids.insert("token".to_string(), Program::token().id());
        program_ids.insert("pinata".to_string(), Program::pinata().id());
        program_ids.insert(
            "privacy_preserving_circuit".to_string(),
            nssa::PRIVACY_PRESERVING_CIRCUIT_ID,
        );
        let helperstruct = GetProgramIdsResponse { program_ids };
        respond(helperstruct)
    }

    pub async fn process_request_internal(&self, request: Request) -> Result<Value, RpcErr> {
        match request.method.as_ref() {
            HELLO => self.process_temp_hello(request).await,
            SEND_TX => self.process_send_tx(request).await,
            GET_BLOCK => self.process_get_block_data(request).await,
            GET_GENESIS => self.process_get_genesis(request).await,
            GET_LAST_BLOCK => self.process_get_last_block(request).await,
            GET_INITIAL_TESTNET_ACCOUNTS => self.get_initial_testnet_accounts(request).await,
            GET_ACCOUNT_BALANCE => self.process_get_account_balance(request).await,
            GET_ACCOUNTS_NONCES => self.process_get_accounts_nonces(request).await,
            GET_ACCOUNT => self.process_get_account(request).await,
            GET_TRANSACTION_BY_HASH => self.process_get_transaction_by_hash(request).await,
            GET_PROOF_FOR_COMMITMENT => self.process_get_proof_by_commitment(request).await,
            GET_PROGRAM_IDS => self.process_get_program_ids(request).await,
            _ => Err(RpcErr(RpcError::method_not_found(request.method))),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{JsonHandler, rpc_handler};
    use base64::{Engine, engine::general_purpose};
    use common::{
        rpc_primitives::RpcPollingConfig, test_utils::sequencer_sign_key_for_testing,
        transaction::EncodedTransaction,
    };

    use sequencer_core::{
        SequencerCore,
        config::{AccountInitialData, SequencerConfig},
    };
    use serde_json::Value;
    use tempfile::tempdir;
    use tokio::sync::Mutex;

    fn sequencer_config_for_tests() -> SequencerConfig {
        let tempdir = tempdir().unwrap();
        let home = tempdir.path().to_path_buf();
        let acc1_addr = vec![
            14, 238, 36, 40, 114, 150, 186, 85, 39, 143, 30, 84, 3, 190, 1, 71, 84, 134, 99, 102,
            56, 135, 48, 48, 60, 40, 137, 190, 23, 173, 160, 101,
        ];

        let acc2_addr = vec![
            158, 61, 142, 101, 77, 68, 14, 149, 41, 58, 162, 220, 236, 235, 19, 120, 153, 165, 149,
            53, 233, 82, 247, 71, 6, 142, 122, 14, 227, 9, 101, 242,
        ];

        let initial_acc1 = AccountInitialData {
            addr: hex::encode(acc1_addr),
            balance: 10000,
        };

        let initial_acc2 = AccountInitialData {
            addr: hex::encode(acc2_addr),
            balance: 20000,
        };

        let initial_accounts = vec![initial_acc1, initial_acc2];

        SequencerConfig {
            home,
            override_rust_log: Some("info".to_string()),
            genesis_id: 1,
            is_genesis_random: false,
            max_num_tx_in_block: 10,
            block_create_timeout_millis: 1000,
            port: 8080,
            initial_accounts,
            initial_commitments: vec![],
            signing_key: *sequencer_sign_key_for_testing().value(),
        }
    }

    fn components_for_tests() -> (JsonHandler, Vec<AccountInitialData>, EncodedTransaction) {
        let config = sequencer_config_for_tests();
        let mut sequencer_core = SequencerCore::start_from_config(config);
        let initial_accounts = sequencer_core.sequencer_config.initial_accounts.clone();

        let signing_key = nssa::PrivateKey::try_new([1; 32]).unwrap();
        let balance_to_move = 10;
        let tx = common::test_utils::create_transaction_native_token_transfer(
            [
                14, 238, 36, 40, 114, 150, 186, 85, 39, 143, 30, 84, 3, 190, 1, 71, 84, 134, 99,
                102, 56, 135, 48, 48, 60, 40, 137, 190, 23, 173, 160, 101,
            ],
            0,
            [2; 32],
            balance_to_move,
            signing_key,
        );

        sequencer_core
            .push_tx_into_mempool_pre_check(tx.clone())
            .unwrap();

        sequencer_core
            .produce_new_block_with_mempool_transactions()
            .unwrap();

        let sequencer_core = Arc::new(Mutex::new(sequencer_core));

        (
            JsonHandler {
                polling_config: RpcPollingConfig::default(),
                sequencer_state: sequencer_core,
            },
            initial_accounts,
            tx,
        )
    }

    async fn call_rpc_handler_with_json(handler: JsonHandler, request_json: Value) -> Value {
        use actix_web::{App, test, web};

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(handler))
                .route("/", web::post().to(rpc_handler)),
        )
        .await;

        let req = test::TestRequest::post()
            .uri("/")
            .set_json(request_json)
            .to_request();

        let resp = test::call_service(&app, req).await;
        let body = test::read_body(resp).await;

        serde_json::from_slice(&body).unwrap()
    }

    #[actix_web::test]
    async fn test_get_account_balance_for_non_existent_account() {
        let (json_handler, _, _) = components_for_tests();
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "get_account_balance",
            "params": { "address": "efac".repeat(16) },
            "id": 1
        });
        let expected_response = serde_json::json!({
            "id": 1,
            "jsonrpc": "2.0",
            "result": {
                "balance": 0
            }
        });

        let response = call_rpc_handler_with_json(json_handler, request).await;

        assert_eq!(response, expected_response);
    }

    #[actix_web::test]
    async fn test_get_account_balance_for_invalid_hex() {
        let (json_handler, _, _) = components_for_tests();
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "get_account_balance",
            "params": { "address": "not_a_valid_hex" },
            "id": 1
        });
        let expected_response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": -32602,
                "message": "Invalid params",
                "data": "invalid hex"
            }
        });
        let response = call_rpc_handler_with_json(json_handler, request).await;

        assert_eq!(response, expected_response);
    }

    #[actix_web::test]
    async fn test_get_account_balance_for_invalid_length() {
        let (json_handler, _, _) = components_for_tests();
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "get_account_balance",
            "params": { "address": "cafecafe" },
            "id": 1
        });
        let expected_response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": -32602,
                "message": "Invalid params",
                "data": "invalid length"
            }
        });
        let response = call_rpc_handler_with_json(json_handler, request).await;

        assert_eq!(response, expected_response);
    }

    #[actix_web::test]
    async fn test_get_account_balance_for_existing_account() {
        let (json_handler, initial_accounts, _) = components_for_tests();

        let acc1_addr = initial_accounts[0].addr.clone();

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "get_account_balance",
            "params": { "address": acc1_addr },
            "id": 1
        });
        let expected_response = serde_json::json!({
            "id": 1,
            "jsonrpc": "2.0",
            "result": {
                "balance": 10000 - 10
            }
        });

        let response = call_rpc_handler_with_json(json_handler, request).await;

        assert_eq!(response, expected_response);
    }

    #[actix_web::test]
    async fn test_get_accounts_nonces_for_non_existent_account() {
        let (json_handler, _, _) = components_for_tests();
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "get_accounts_nonces",
            "params": { "addresses": ["efac".repeat(16)] },
            "id": 1
        });
        let expected_response = serde_json::json!({
            "id": 1,
            "jsonrpc": "2.0",
            "result": {
                "nonces": [ 0 ]
            }
        });

        let response = call_rpc_handler_with_json(json_handler, request).await;

        assert_eq!(response, expected_response);
    }

    #[actix_web::test]
    async fn test_get_accounts_nonces_for_existent_account() {
        let (json_handler, initial_accounts, _) = components_for_tests();

        let acc_1_addr = initial_accounts[0].addr.clone();
        let acc_2_addr = initial_accounts[1].addr.clone();

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "get_accounts_nonces",
            "params": { "addresses": [acc_1_addr, acc_2_addr] },
            "id": 1
        });
        let expected_response = serde_json::json!({
            "id": 1,
            "jsonrpc": "2.0",
            "result": {
                "nonces": [ 1, 0 ]
            }
        });

        let response = call_rpc_handler_with_json(json_handler, request).await;

        assert_eq!(response, expected_response);
    }

    #[actix_web::test]
    async fn test_get_account_data_for_non_existent_account() {
        let (json_handler, _, _) = components_for_tests();
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "get_account",
            "params": { "address": "efac".repeat(16) },
            "id": 1
        });
        let expected_response = serde_json::json!({
            "id": 1,
            "jsonrpc": "2.0",
            "result": {
                "account": {
                    "balance": 0,
                    "nonce": 0,
                    "program_owner": [ 0, 0, 0, 0, 0, 0, 0, 0],
                    "data": [],
                }
            }
        });

        let response = call_rpc_handler_with_json(json_handler, request).await;

        assert_eq!(response, expected_response);
    }

    #[actix_web::test]
    async fn test_get_transaction_by_hash_for_non_existent_hash() {
        let (json_handler, _, _) = components_for_tests();
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "get_transaction_by_hash",
            "params": { "hash": "cafe".repeat(16) },
            "id": 1
        });
        let expected_response = serde_json::json!({
            "id": 1,
            "jsonrpc": "2.0",
            "result": {
                "transaction": null
            }
        });

        let response = call_rpc_handler_with_json(json_handler, request).await;

        assert_eq!(response, expected_response);
    }

    #[actix_web::test]
    async fn test_get_transaction_by_hash_for_invalid_hex() {
        let (json_handler, _, _) = components_for_tests();
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "get_transaction_by_hash",
            "params": { "hash": "not_a_valid_hex" },
            "id": 1
        });
        let expected_response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": -32602,
                "message": "Invalid params",
                "data": "invalid hex"
            }
        });

        let response = call_rpc_handler_with_json(json_handler, request).await;

        assert_eq!(response, expected_response);
    }

    #[actix_web::test]
    async fn test_get_transaction_by_hash_for_invalid_length() {
        let (json_handler, _, _) = components_for_tests();
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "get_transaction_by_hash",
            "params": { "hash": "cafecafe" },
            "id": 1
        });
        let expected_response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": -32602,
                "message": "Invalid params",
                "data": "invalid length"
            }
        });

        let response = call_rpc_handler_with_json(json_handler, request).await;

        assert_eq!(response, expected_response);
    }

    #[actix_web::test]
    async fn test_get_transaction_by_hash_for_existing_transaction() {
        let (json_handler, _, tx) = components_for_tests();
        let tx_hash_hex = hex::encode(tx.hash());
        let expected_base64_encoded = general_purpose::STANDARD.encode(borsh::to_vec(&tx).unwrap());

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "get_transaction_by_hash",
            "params": { "hash": tx_hash_hex},
            "id": 1
        });

        let expected_response = serde_json::json!({
            "id": 1,
            "jsonrpc": "2.0",
            "result": {
                "transaction": expected_base64_encoded,
            }
        });
        let response = call_rpc_handler_with_json(json_handler, request).await;

        assert_eq!(response, expected_response);
    }
}

use actix_web::Error as HttpError;
use serde_json::Value;

use common::rpc_primitives::{
    errors::RpcError,
    message::{Message, Request},
    parser::RpcRequest,
    requests::{GetAccountBalanceRequest, GetAccountBalanceResponse},
};

use common::rpc_primitives::requests::{
    GetBlockDataRequest, GetBlockDataResponse, GetGenesisIdRequest, GetGenesisIdResponse,
    GetLastBlockRequest, GetLastBlockResponse, HelloRequest, HelloResponse, RegisterAccountRequest,
    RegisterAccountResponse, SendTxRequest, SendTxResponse,
};

use super::{respond, types::err_rpc::RpcErr, JsonHandler};

pub const HELLO: &str = "hello";
pub const REGISTER_ACCOUNT: &str = "register_account";
pub const SEND_TX: &str = "send_tx";
pub const GET_BLOCK: &str = "get_block";
pub const GET_GENESIS: &str = "get_genesis";
pub const GET_LAST_BLOCK: &str = "get_last_block";
pub const GET_ACCOUNT_BALANCE: &str = "get_account_balance";

pub const HELLO_FROM_SEQUENCER: &str = "HELLO_FROM_SEQUENCER";

pub const SUCCESS: &str = "Success";

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

    async fn process_register_account_request(&self, request: Request) -> Result<Value, RpcErr> {
        let acc_req = RegisterAccountRequest::parse(Some(request.params))?;

        {
            let mut acc_store = self.sequencer_state.lock().await;

            acc_store.register_account(acc_req.address);
        }

        let helperstruct = RegisterAccountResponse {
            status: SUCCESS.to_string(),
        };

        respond(helperstruct)
    }

    async fn process_send_tx(&self, request: Request) -> Result<Value, RpcErr> {
        let send_tx_req = SendTxRequest::parse(Some(request.params))?;

        {
            let mut state = self.sequencer_state.lock().await;

            state.push_tx_into_mempool_pre_check(send_tx_req.transaction, send_tx_req.tx_roots)?;
        }

        let helperstruct = SendTxResponse {
            status: SUCCESS.to_string(),
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

        let helperstruct = GetBlockDataResponse { block };

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

    /// Returns the balance of the account at the given address.
    /// The address must be a valid hex string of the correct length.
    async fn process_get_account_balance(&self, request: Request) -> Result<Value, RpcErr> {
        let get_account_req = GetAccountBalanceRequest::parse(Some(request.params))?;
        let address_bytes = hex::decode(get_account_req.address)
            .map_err(|_| RpcError::invalid_params("invalid hex".to_string()))?;
        let address = address_bytes
            .try_into()
            .map_err(|_| RpcError::invalid_params("invalid length".to_string()))?;
        let balance = {
            let state = self.sequencer_state.lock().await;
            state.store.acc_store.get_account_balance(&address)
        }
        .unwrap_or(0);

        let helperstruct = GetAccountBalanceResponse { balance };

        respond(helperstruct)
    }

    pub async fn process_request_internal(&self, request: Request) -> Result<Value, RpcErr> {
        match request.method.as_ref() {
            HELLO => self.process_temp_hello(request).await,
            REGISTER_ACCOUNT => self.process_register_account_request(request).await,
            SEND_TX => self.process_send_tx(request).await,
            GET_BLOCK => self.process_get_block_data(request).await,
            GET_GENESIS => self.process_get_genesis(request).await,
            GET_LAST_BLOCK => self.process_get_last_block(request).await,
            GET_ACCOUNT_BALANCE => self.process_get_account_balance(request).await,
            _ => Err(RpcErr(RpcError::method_not_found(request.method))),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{rpc_handler, JsonHandler};
    use common::rpc_primitives::RpcPollingConfig;
    use sequencer_core::{
        config::{AccountInitialData, SequencerConfig},
        SequencerCore,
    };
    use serde_json::Value;
    use tempfile::tempdir;
    use tokio::sync::Mutex;

    fn sequencer_config_for_tests() -> SequencerConfig {
        let tempdir = tempdir().unwrap();
        let home = tempdir.path().to_path_buf();
        let initial_accounts = vec![
            AccountInitialData {
                addr: "cafe".repeat(16).to_string(),
                balance: 100,
            },
            AccountInitialData {
                addr: "feca".repeat(16).to_string(),
                balance: 200,
            },
        ];

        SequencerConfig {
            home,
            override_rust_log: Some("info".to_string()),
            genesis_id: 1,
            is_genesis_random: false,
            max_num_tx_in_block: 10,
            block_create_timeout_millis: 1000,
            port: 8080,
            initial_accounts,
        }
    }

    fn json_handler_for_tests() -> JsonHandler {
        let config = sequencer_config_for_tests();
        let sequencer_core = Arc::new(Mutex::new(SequencerCore::start_from_config(config)));

        JsonHandler {
            polling_config: RpcPollingConfig::default(),
            sequencer_state: sequencer_core,
        }
    }

    async fn call_rpc_handler_with_json(handler: JsonHandler, request_json: Value) -> Value {
        use actix_web::{test, web, App};

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
        let json_handler = json_handler_for_tests();
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
        let json_handler = json_handler_for_tests();
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
        let json_handler = json_handler_for_tests();
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
        let json_handler = json_handler_for_tests();
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "get_account_balance",
            "params": { "address": "cafe".repeat(16) },
            "id": 1
        });
        let expected_response = serde_json::json!({
            "id": 1,
            "jsonrpc": "2.0",
            "result": {
                "balance": 100
            }
        });

        let response = call_rpc_handler_with_json(json_handler, request).await;

        assert_eq!(response, expected_response);
    }
}
